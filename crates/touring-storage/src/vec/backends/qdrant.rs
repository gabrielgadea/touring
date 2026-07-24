//! Qdrant backend for touring-vector-store.
//!
//! Uses the qdrant-client crate for gRPC communication with a Qdrant server.

use crate::vec::{CollectionSchema, DistanceMetric, Point, SearchHit, SearchQuery, VectorStore};
use async_trait::async_trait;
use qdrant_client::Qdrant;
use qdrant_client::qdrant::{
    CollectionExistsRequest, Condition, CreateCollectionBuilder, DeletePointsBuilder,
    Distance as QdrantDistance, Filter, PointId, PointStruct, SearchPointsBuilder,
    UpsertPointsBuilder, VectorParamsBuilder,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Cache entry for a collection schema retrieved from Qdrant.
#[derive(Debug, Clone)]
struct CollectionCacheEntry {
    dimension: usize,
    distance: QdrantDistance,
}

/// Qdrant backend for vector storage.
///
/// Wraps a `Qdrant` client and caches collection schemas to avoid
/// repeated dimension lookups.
#[derive(Clone)]
pub struct QdrantStore {
    client: Qdrant,
    /// Cache of known collection schemas (dimension + distance).
    /// Key: collection name.
    schema_cache: Arc<RwLock<HashMap<String, CollectionCacheEntry>>>,
}

impl QdrantStore {
    /// Create a new QdrantStore connected to the given URL.
    pub fn new(url: &str) -> Result<Self, crate::vec::VectorStoreError> {
        let client = Qdrant::from_url(url)
            .build()
            .map_err(|e| crate::vec::VectorStoreError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            client,
            schema_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new QdrantStore connected to the given URL with API key auth.
    pub fn with_api_key(url: &str, api_key: &str) -> Result<Self, crate::vec::VectorStoreError> {
        let client = Qdrant::from_url(url)
            .api_key(api_key)
            .build()
            .map_err(|e| crate::vec::VectorStoreError::ConnectionFailed(e.to_string()))?;

        Ok(Self {
            client,
            schema_cache: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Convert a `DistanceMetric` to a Qdrant `Distance`.
    fn to_qdrant_distance(metric: DistanceMetric) -> QdrantDistance {
        match metric {
            DistanceMetric::Cosine => QdrantDistance::Cosine,
            DistanceMetric::Euclidean => QdrantDistance::Euclid,
            DistanceMetric::DotProduct => QdrantDistance::Dot,
        }
    }

    /// Render a Qdrant `PointId` (numeric or UUID) as a plain string.
    fn point_id_to_string(id: PointId) -> String {
        use qdrant_client::qdrant::point_id::PointIdOptions;
        match id.point_id_options {
            Some(PointIdOptions::Num(n)) => n.to_string(),
            Some(PointIdOptions::Uuid(s)) => s,
            None => String::new(),
        }
    }

    /// Convert a `Point` id string into a Qdrant `PointId`.
    ///
    /// Qdrant only accepts unsigned-integer or UUID point ids. A purely numeric
    /// id string maps to the `Num` variant (so a `"42"` id is stored as the
    /// integer 42 and is accepted by the server); any other string maps to the
    /// `Uuid` variant. This is the write-side mirror of [`point_id_to_string`],
    /// which already renders both `Num` and `Uuid` — so a numeric id survives a
    /// full `upsert` → `search` round-trip unchanged.
    fn string_id_to_point_id(id: &str) -> PointId {
        match id.parse::<u64>() {
            Ok(n) => PointId::from(n),
            Err(_) => PointId::from(id.to_string()),
        }
    }

    /// Convert a Qdrant `Distance` back to a `DistanceMetric`.
    fn from_qdrant_distance(distance: QdrantDistance) -> DistanceMetric {
        match distance {
            QdrantDistance::Euclid => DistanceMetric::Euclidean,
            QdrantDistance::Dot => DistanceMetric::DotProduct,
            // Cosine — and any metric without a `DistanceMetric` equivalent.
            _ => DistanceMetric::Cosine,
        }
    }

    /// Builds a Qdrant `Filter` from a JSON object of field→value equality pairs.
    ///
    /// Each string-valued entry (`{"lang": "rust"}`) becomes a keyword-match
    /// condition; the conditions are ANDed via `Filter::must`. Returns `None`
    /// when the JSON is not an object or carries no string values — callers
    /// then issue an unfiltered search.
    fn json_to_filter(filter_json: &serde_json::Value) -> Option<Filter> {
        let obj = filter_json.as_object()?;
        let conditions: Vec<Condition> = obj
            .iter()
            .filter_map(|(field, value)| {
                value
                    .as_str()
                    .map(|s| Condition::matches(field.clone(), s.to_string()))
            })
            .collect();
        if conditions.is_empty() {
            None
        } else {
            Some(Filter::must(conditions))
        }
    }

    /// Returns the `(dimension, distance_metric)` schema of a collection.
    ///
    /// The schema is fetched from Qdrant on the first call and cached, so
    /// repeated lookups are served from memory.
    ///
    /// # Errors
    /// Returns an error if the collection is missing or the daemon is
    /// unreachable.
    pub async fn collection_schema(
        &self,
        collection_name: &str,
    ) -> Result<(usize, DistanceMetric), crate::vec::VectorStoreError> {
        let entry = self.get_schema_cached(collection_name).await?;
        Ok((entry.dimension, Self::from_qdrant_distance(entry.distance)))
    }

    /// Get or cache the schema for a collection.
    async fn get_schema_cached(
        &self,
        collection_name: &str,
    ) -> Result<CollectionCacheEntry, crate::vec::VectorStoreError> {
        // Check cache first
        {
            let cache = self.schema_cache.read().await;
            if let Some(entry) = cache.get(collection_name) {
                return Ok(entry.clone());
            }
        }

        // Fetch from Qdrant — collection_info returns collection description
        let info = self
            .client
            .collection_info(collection_name)
            .await
            .map_err(|e| crate::vec::VectorStoreError::ConnectionFailed(e.to_string()))?;

        let collection = info.result.ok_or_else(|| {
            crate::vec::VectorStoreError::CollectionNotFound(collection_name.to_string())
        })?;

        let config = collection.config.as_ref().ok_or_else(|| {
            crate::vec::VectorStoreError::ConnectionFailed(format!(
                "collection '{collection_name}' has no config"
            ))
        })?;
        let params = config.params.as_ref().ok_or_else(|| {
            crate::vec::VectorStoreError::ConnectionFailed(format!(
                "collection '{collection_name}' has no params"
            ))
        })?;

        // CollectionParams 1.17 nests vector geometry under
        // `vectors_config.config` — a `Params(VectorParams)` oneof.
        let vector_params = params
            .vectors_config
            .as_ref()
            .and_then(|vc| vc.config.as_ref())
            .and_then(|cfg| match cfg {
                qdrant_client::qdrant::vectors_config::Config::Params(p) => Some(p),
                qdrant_client::qdrant::vectors_config::Config::ParamsMap(_) => None,
            });

        let dimension = vector_params.map(|p| p.size as usize).unwrap_or(0);
        let distance = vector_params
            .and_then(|p| QdrantDistance::try_from(p.distance).ok())
            .unwrap_or(QdrantDistance::Cosine);

        let entry = CollectionCacheEntry {
            dimension,
            distance,
        };

        // Cache it
        {
            let mut cache = self.schema_cache.write().await;
            cache.insert(collection_name.to_string(), entry.clone());
        }

        Ok(entry)
    }

    /// Invalidate cached schema for a collection.
    async fn invalidate_cache(&self, collection_name: &str) {
        let mut cache = self.schema_cache.write().await;
        cache.remove(collection_name);
    }
}

#[async_trait]
impl VectorStore for QdrantStore {
    async fn collection_exists(&self, name: &str) -> Result<bool, crate::vec::VectorStoreError> {
        let request = CollectionExistsRequest {
            collection_name: name.to_string(),
        };
        let exists = self
            .client
            .collection_exists(request)
            .await
            .map_err(|e| crate::vec::VectorStoreError::ConnectionFailed(e.to_string()))?;
        Ok(exists)
    }

    async fn create_collection(
        &self,
        schema: CollectionSchema,
    ) -> Result<(), crate::vec::VectorStoreError> {
        let request = CreateCollectionBuilder::new(schema.name.clone())
            .vectors_config(VectorParamsBuilder::new(
                schema.dimension as u64,
                Self::to_qdrant_distance(schema.distance),
            ))
            .build();

        self.client
            .create_collection(request)
            .await
            .map_err(|e| crate::vec::VectorStoreError::UpsertFailed(e.to_string()))?;

        // Populate cache
        {
            let mut cache = self.schema_cache.write().await;
            cache.insert(
                schema.name.clone(),
                CollectionCacheEntry {
                    dimension: schema.dimension,
                    distance: Self::to_qdrant_distance(schema.distance),
                },
            );
        }

        Ok(())
    }

    async fn delete_collection(&self, name: &str) -> Result<(), crate::vec::VectorStoreError> {
        self.client
            .delete_collection(name)
            .await
            .map_err(|e| crate::vec::VectorStoreError::DeleteFailed(e.to_string()))?;

        self.invalidate_cache(name).await;
        Ok(())
    }

    async fn upsert(
        &self,
        collection_name: &str,
        points: Vec<Point>,
    ) -> Result<(), crate::vec::VectorStoreError> {
        let qdrant_points: Vec<PointStruct> = points
            .into_iter()
            .map(|p| {
                let payload = if p.metadata.is_null() {
                    qdrant_client::Payload::default()
                } else {
                    p.metadata
                        .try_into()
                        .unwrap_or_else(|_| qdrant_client::Payload::default())
                };
                PointStruct::new(Self::string_id_to_point_id(&p.id), p.vector, payload)
            })
            .collect();

        // `wait(true)` makes the upsert synchronous: the points are indexed
        // and searchable by the time this returns, matching the contract the
        // in-memory and sqlite-vec backends already honour.
        let request = UpsertPointsBuilder::new(collection_name, qdrant_points)
            .wait(true)
            .build();

        self.client
            .upsert_points(request)
            .await
            .map_err(|e| crate::vec::VectorStoreError::UpsertFailed(e.to_string()))?;

        Ok(())
    }

    async fn search(
        &self,
        collection_name: &str,
        query: SearchQuery,
    ) -> Result<Vec<SearchHit>, crate::vec::VectorStoreError> {
        let mut builder =
            SearchPointsBuilder::new(collection_name, query.vector, query.top_k as u64);

        if query.with_metadata {
            builder = builder.with_payload(true);
        }

        // A JSON object of field→string pairs becomes ANDed keyword-match
        // conditions; non-object or value-less filters yield an unfiltered search.
        if let Some(filter) = query.filter.as_ref().and_then(Self::json_to_filter) {
            builder = builder.filter(filter);
        }

        let request = builder.build();

        let results = self
            .client
            .search_points(request)
            .await
            .map_err(|e| crate::vec::VectorStoreError::SearchFailed(e.to_string()))?;

        let hits = results
            .result
            .into_iter()
            .map(|scored_point| {
                let id = scored_point
                    .id
                    .map(Self::point_id_to_string)
                    .unwrap_or_default();

                let metadata = if query.with_metadata {
                    serde_json::to_value(&scored_point.payload).unwrap_or(serde_json::Value::Null)
                } else {
                    serde_json::Value::Null
                };

                SearchHit {
                    id,
                    score: scored_point.score,
                    metadata,
                }
            })
            .collect();

        Ok(hits)
    }

    async fn delete(
        &self,
        collection_name: &str,
        ids: Vec<String>,
    ) -> Result<(), crate::vec::VectorStoreError> {
        // Map each id string through `string_id_to_point_id` — the same
        // conversion `upsert` applies — so a point stored under the numeric
        // id "1" (kept as a `Num` PointId) can be addressed for deletion.
        // Without this, `Vec<String>` ids reach Qdrant as invalid UUIDs.
        let point_ids: Vec<PointId> = ids
            .iter()
            .map(|id| Self::string_id_to_point_id(id))
            .collect();
        // `wait(true)` makes the delete synchronous, so a subsequent search
        // never observes a point that this call already removed.
        let request = DeletePointsBuilder::new(collection_name)
            .points(point_ids)
            .wait(true)
            .build();

        self.client
            .delete_points(request)
            .await
            .map_err(|e| crate::vec::VectorStoreError::DeleteFailed(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_round_trips_through_qdrant_and_back() {
        for metric in [
            DistanceMetric::Cosine,
            DistanceMetric::Euclidean,
            DistanceMetric::DotProduct,
        ] {
            let qd = QdrantStore::to_qdrant_distance(metric);
            assert_eq!(
                QdrantStore::from_qdrant_distance(qd),
                metric,
                "DistanceMetric round-trip must be identity for {metric:?}"
            );
        }
    }

    #[test]
    fn from_qdrant_distance_maps_known_variants() {
        assert_eq!(
            QdrantStore::from_qdrant_distance(QdrantDistance::Euclid),
            DistanceMetric::Euclidean
        );
        assert_eq!(
            QdrantStore::from_qdrant_distance(QdrantDistance::Dot),
            DistanceMetric::DotProduct
        );
        assert_eq!(
            QdrantStore::from_qdrant_distance(QdrantDistance::Cosine),
            DistanceMetric::Cosine
        );
    }

    #[test]
    fn point_id_to_string_renders_numeric_uuid_and_empty() {
        use qdrant_client::qdrant::point_id::PointIdOptions;
        let numeric = PointId {
            point_id_options: Some(PointIdOptions::Num(42)),
        };
        assert_eq!(QdrantStore::point_id_to_string(numeric), "42");

        let uuid = PointId {
            point_id_options: Some(PointIdOptions::Uuid("abc-123".to_string())),
        };
        assert_eq!(QdrantStore::point_id_to_string(uuid), "abc-123");

        let empty = PointId {
            point_id_options: None,
        };
        assert_eq!(QdrantStore::point_id_to_string(empty), "");
    }

    #[test]
    fn string_id_to_point_id_round_trips_through_point_id_to_string() {
        // A numeric id string survives upsert → search unchanged.
        let numeric = QdrantStore::string_id_to_point_id("42");
        assert_eq!(QdrantStore::point_id_to_string(numeric), "42");

        // A UUID id string survives the round-trip unchanged.
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let pid = QdrantStore::string_id_to_point_id(uuid);
        assert_eq!(QdrantStore::point_id_to_string(pid), uuid);
    }

    #[test]
    fn string_id_to_point_id_selects_num_or_uuid_variant() {
        use qdrant_client::qdrant::point_id::PointIdOptions;
        // A purely numeric id string maps to the Num variant.
        let numeric = QdrantStore::string_id_to_point_id("7");
        assert!(
            matches!(numeric.point_id_options, Some(PointIdOptions::Num(7))),
            "a numeric id string must map to the Num variant"
        );
        // Any other id string falls back to the Uuid variant.
        let other = QdrantStore::string_id_to_point_id("doc-xyz");
        assert!(
            matches!(other.point_id_options, Some(PointIdOptions::Uuid(_))),
            "a non-numeric id string must map to the Uuid variant"
        );
    }

    #[test]
    fn json_to_filter_builds_one_condition_per_string_pair() {
        let json = serde_json::json!({ "lang": "rust", "kind": "fn" });
        let filter =
            QdrantStore::json_to_filter(&json).expect("two string pairs must produce a filter");
        assert_eq!(
            filter.must.len(),
            2,
            "each string pair becomes one must-condition"
        );
    }

    #[test]
    fn json_to_filter_returns_none_for_non_string_empty_and_non_object() {
        // Non-string values are dropped — nothing left → None.
        let non_string = serde_json::json!({ "count": 42, "flag": true });
        assert!(
            QdrantStore::json_to_filter(&non_string).is_none(),
            "object with no string values → None"
        );
        // Empty object → None.
        let empty = serde_json::json!({});
        assert!(
            QdrantStore::json_to_filter(&empty).is_none(),
            "empty object → None"
        );
        // Non-object JSON → None.
        let scalar = serde_json::json!("plain string");
        assert!(
            QdrantStore::json_to_filter(&scalar).is_none(),
            "non-object JSON → None"
        );
    }
}

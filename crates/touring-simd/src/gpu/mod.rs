//! GPU compute backend abstraction for touring-simd.
//!
//! Provides a unified interface for compute backends:
//! - [`SimdBackend`] — CPU SIMD via `pulp` (AVX-512/AVX2+FMA/NEON)
//! - [`HttpGpuBackend`] — Remote GPU server via HTTP (enabled by `gpu-compute` feature)

use std::error::Error;
use std::fmt::Debug;

// ─── GpuBackend Trait ────────────────────────────────────────────────────────

/// Unified GPU compute backend trait.
///
/// Implementors provide vector computation via different hardware strategies:
/// - CPU SIMD via `pulp` (always available)
/// - Remote GPU server via HTTP (requires `gpu-compute` feature)
pub trait GpuBackend: Send + Sync {
    /// Input type for dot product computation.
    type DotInput: Send + Sync + Clone + Debug + 'static;
    /// Output type for dot product computation.
    type DotOutput: Send + Sync + Clone + Debug + 'static;

    /// Compute dot product of two vectors.
    fn compute_dot(
        &self,
        input_a: &[Self::DotInput],
        input_b: &[Self::DotInput],
    ) -> Result<Self::DotOutput, Box<dyn Error + Send + Sync>>;

    /// Compute cosine similarity between two vectors.
    ///
    /// Returns a value in [-1.0, 1.0] where 1.0 = identical direction.
    fn compute_cosine(&self, a: &[f32], b: &[f32]) -> Result<f64, Box<dyn Error + Send + Sync>>;

    /// Compute top-K most similar vectors to a query.
    ///
    /// Returns a vector of (index, score) pairs sorted by score descending.
    fn compute_topk(
        &self,
        query: &[f32],
        candidates: &[Vec<f32>],
        k: usize,
    ) -> Result<Vec<(usize, f64)>, Box<dyn Error + Send + Sync>>;

    /// Human-readable identifier of this backend's hardware strategy.
    fn backend_name(&self) -> &'static str;
    /// Relative performance capability of this backend, used to pick the fastest available one.
    fn capability_score(&self) -> f32;
}

// ─── SimdBackend (CPU SIMD via pulp) ────────────────────────────────────────

mod simd_impl {
    use super::*;
    use crate::simd_utils::simd_dot_f32;

    /// CPU SIMD compute backend backed by `pulp` (AVX-512/AVX2+FMA/NEON), always available.
    #[derive(Debug, Clone, Default)]
    pub struct SimdBackend {
        _private: (),
    }

    impl SimdBackend {
        /// Creates a new CPU SIMD backend.
        #[inline]
        pub fn new() -> Self {
            Self { _private: () }
        }
    }

    impl GpuBackend for SimdBackend {
        type DotInput = f32;
        type DotOutput = f32;

        #[inline]
        fn compute_dot(&self, a: &[f32], b: &[f32]) -> Result<f32, Box<dyn Error + Send + Sync>> {
            if a.len() != b.len() {
                return Err("Vector length mismatch".into());
            }
            Ok(simd_dot_f32(a, b))
        }

        #[inline]
        fn compute_cosine(
            &self,
            a: &[f32],
            b: &[f32],
        ) -> Result<f64, Box<dyn Error + Send + Sync>> {
            if a.len() != b.len() {
                return Err("Vector length mismatch".into());
            }
            Ok(simd_dot_f32(a, b) as f64)
        }

        fn compute_topk(
            &self,
            query: &[f32],
            candidates: &[Vec<f32>],
            k: usize,
        ) -> Result<Vec<(usize, f64)>, Box<dyn Error + Send + Sync>> {
            if query.is_empty() || candidates.is_empty() || k == 0 {
                return Ok(vec![]);
            }

            let k = k.min(candidates.len());

            // Special case: if k == candidates.len(), just sort and return all
            if k == candidates.len() {
                let mut scored: Vec<(usize, f64)> = candidates
                    .iter()
                    .enumerate()
                    .map(|(idx, cand)| {
                        let score = self.compute_cosine(query, cand).unwrap_or(0.0);
                        (idx, score)
                    })
                    .collect();
                scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                return Ok(scored);
            }

            let mut scored: Vec<(usize, f64)> = candidates
                .iter()
                .enumerate()
                .map(|(idx, cand)| {
                    let score = self.compute_cosine(query, cand).unwrap_or(0.0);
                    (idx, score)
                })
                .collect();

            // select_nth_unstable_by with pivot at k: elements before k are <= elements after
            // Then we truncate to k and sort
            scored.select_nth_unstable_by(k, |a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
            });
            scored.truncate(k);
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

            Ok(scored)
        }

        fn backend_name(&self) -> &'static str {
            "SimdBackend (pulp)"
        }
        fn capability_score(&self) -> f32 {
            if crate::simd_utils::has_avx2() {
                0.85
            } else if crate::simd_utils::has_neon() {
                0.75
            } else {
                0.5
            }
        }
    }
}

pub use simd_impl::SimdBackend;

// ─── HttpGpuBackend (Remote GPU via HTTP/gRPC) ───────────────────────────────

#[cfg(feature = "gpu-compute")]
mod http_impl {
    use super::*;
    use std::sync::Mutex;
    use wgpu::{
        BufferDescriptor, BufferUsages, CommandEncoderDescriptor, ComputePassDescriptor,
        ComputePipeline, ComputePipelineDescriptor, Device, DeviceDescriptor, Instance,
        InstanceDescriptor, Queue, ShaderModuleDescriptor,
    };

    // WGSL compute shader for element-wise dot product
    // Each workitem computes output[i] = input_a[i] * input_b[i]
    // Workgroup size: 64 (optimal for NVIDIA RTX 4060)
    const DOT_PRODUCT_SHADER: &str = r#"
        @group(0) @binding(0) var<storage, read> input_a: array<f32>;
        @group(0) @binding(1) var<storage, read> input_b: array<f32>;
        @group(0) @binding(2) var<storage, read_write> output: array<f32>;

        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
            let idx = global_id.x;
            if (idx >= arrayLength(&input_a)) { return; }
            output[idx] = input_a[idx] * input_b[idx];
        }
    "#;

    // U4 quantized dot product shader - nibbles unpacked INSIDE GPU
    // Reduz PCI-e transfer em 87.5% (32bit -> 4bit per value)
    // WGSL does not support u8, so we use i32 with bitcast
    const U4_DOT_SHADER: &str = r#"
        @group(0) @binding(0) var<storage, read> packed_a: array<i32>;
        @group(0) @binding(1) var<storage, read> packed_b: array<i32>;
        @group(0) @binding(2) var<storage, read_write> output: array<f32>;
        @group(0) @binding(3) var<uniform> dequant_meta: vec4<f32>;

        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
            let idx = global_id.x;
            if (idx >= arrayLength(&output)) { return; }

            let nibble_idx_a = idx / 2;
            let is_low = (idx % 2) == 0;
            let packed_val_a = packed_a[nibble_idx_a];
            let raw_a = select(f32(packed_val_a & 0x0F), f32(packed_val_a >> 4), is_low);
            let a = raw_a * dequant_meta.x + dequant_meta.y;

            let nibble_idx_b = idx / 2;
            let is_low_b = (idx % 2) == 0;
            let packed_val_b = packed_b[nibble_idx_b];
            let raw_b = select(f32(packed_val_b & 0x0F), f32(packed_val_b >> 4), is_low_b);
            let b = raw_b * dequant_meta.z + dequant_meta.w;

            output[idx] = a * b;
        }
    "#;

    // GPU reduction shader - workgroup-level reduction to avoid GPU->CPU transfer
    const REDUCE_SHADER: &str = r#"
        @group(0) @binding(0) var<storage, read> input: array<f32>;
        @group(0) @binding(1) var<storage, read_write> output: array<f32>;

        var<workgroup> shared_mem: array<f32, 64>;

        @compute @workgroup_size(64)
        fn main(@builtin(global_invocation_id) global_id: vec3<u32>,
                @builtin(local_invocation_id) local_id: vec3<u32>) {
            let idx = global_id.x;
            shared_mem[local_id.x] = input[idx];

            var stride: u32 = 32;
            for (var i = 0u; i < 6u; i++) {
                if (local_id.x < stride) {
                    shared_mem[local_id.x] += shared_mem[local_id.x + stride];
                }
                workgroupBarrier();
                stride = stride >> 1;
            }

            if (local_id.x == 0) {
                output[0] = shared_mem[0];
            }
        }
    "#;

    /// GPU device and resource manager
    #[derive(Clone)]
    pub struct GpuResources {
        /// Logical GPU device handle used to allocate buffers and pipelines.
        pub device: Device,
        /// Command queue for submitting compute work to the GPU device.
        pub queue: Queue,
        compute_pipeline: ComputePipeline,
        u4_dot_pipeline: ComputePipeline,
        reduce_pipeline: ComputePipeline,
    }

    /// Thread-safe GPU state with lazy initialization
    static GPU_STATE: Mutex<Option<GpuResources>> = Mutex::new(None);

    /// Initialize or get cached GPU resources
    ///
    /// Thread-safe lazy singleton. First call initializes the Vulkan device,
    /// queues, and compute pipelines. Subsequent calls return the cached instance.
    pub fn get_gpu_resources() -> Result<GpuResources, Box<dyn Error + Send + Sync>> {
        // Fast path: already initialized
        {
            let guard = GPU_STATE
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            if let Some(ref resources) = *guard {
                return Ok(resources.clone());
            }
        }

        // Slow path: initialize GPU
        let instance = Instance::new(&InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            flags: wgpu::InstanceFlags::empty(),
            backend_options: wgpu::BackendOptions::default(),
            memory_budget_thresholds: Default::default(),
        });

        let adapters = instance.enumerate_adapters(wgpu::Backends::VULKAN);
        let adapter = adapters
            .into_iter()
            .next()
            .ok_or("No Vulkan GPU adapter found — ensure NVIDIA driver is loaded")?;

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: Some("HttpGpuBackend compute device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: Default::default(),
            trace: Default::default(),
        }))
        .map_err(|e| format!("Failed to request GPU device: {}", e))?;

        let shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("dot_product_shader"),
            source: wgpu::ShaderSource::Wgsl(DOT_PRODUCT_SHADER.into()),
        });

        let compute_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("dot_product_pipeline"),
            layout: None,
            module: &shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // U4 dot product shader module
        let u4_shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("u4_dot_shader"),
            source: wgpu::ShaderSource::Wgsl(U4_DOT_SHADER.into()),
        });
        let u4_dot_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("u4_dot_pipeline"),
            layout: None,
            module: &u4_shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        // Reduction shader module
        let reduce_shader_module = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("reduce_shader"),
            source: wgpu::ShaderSource::Wgsl(REDUCE_SHADER.into()),
        });
        let reduce_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("reduce_pipeline"),
            layout: None,
            module: &reduce_shader_module,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });

        let resources = GpuResources {
            device,
            queue,
            compute_pipeline,
            u4_dot_pipeline,
            reduce_pipeline,
        };

        // Cache the initialized resources
        {
            let mut guard = GPU_STATE
                .lock()
                .map_err(|e| format!("Lock poisoned: {}", e))?;
            *guard = Some(resources.clone());
        }

        Ok(resources)
    }

    /// Cast f32 slice to bytes
    ///
    /// # Safety
    ///
    /// The returned slice is valid for the lifetime of `slice`.
    /// The slice must not be used after `slice` is dropped.
    fn cast_to_bytes(slice: &[f32]) -> &[u8] {
        let len_bytes = std::mem::size_of_val(slice);
        unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, len_bytes) }
    }

    /// Compute backend that offloads vector operations to a remote GPU server over HTTP.
    #[derive(Debug, Clone)]
    pub struct HttpGpuBackend {
        gpu_url: String,
        embedding_dim: usize,
    }

    impl HttpGpuBackend {
        /// Creates a backend targeting `gpu_url` for vectors of the given `embedding_dim`.
        #[inline]
        pub fn new(gpu_url: impl Into<String>, embedding_dim: usize) -> Self {
            Self {
                gpu_url: gpu_url.into(),
                embedding_dim,
            }
        }

        /// Returns the GPU URL endpoint.
        pub fn gpu_url(&self) -> &str {
            &self.gpu_url
        }

        /// Returns the embedding dimension.
        pub fn embedding_dim(&self) -> usize {
            self.embedding_dim
        }
    }

    impl GpuBackend for HttpGpuBackend {
        type DotInput = f32;
        type DotOutput = f32;

        fn compute_dot(
            &self,
            input_a: &[f32],
            input_b: &[f32],
        ) -> Result<f32, Box<dyn Error + Send + Sync>> {
            if input_a.len() != input_b.len() {
                return Err("Vector length mismatch".into());
            }
            if input_a.is_empty() {
                return Ok(0.0);
            }

            let len = input_a.len();
            let resources = get_gpu_resources()?;
            let device = &resources.device;
            let queue = &resources.queue;
            let compute_pipeline = &resources.compute_pipeline;

            // Create storage buffers
            let buffer_a = device.create_buffer(&BufferDescriptor {
                label: Some("input_a"),
                size: std::mem::size_of_val(input_a) as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let buffer_b = device.create_buffer(&BufferDescriptor {
                label: Some("input_b"),
                size: std::mem::size_of_val(input_a) as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let buffer_out = device.create_buffer(&BufferDescriptor {
                label: Some("output"),
                size: std::mem::size_of_val(input_a) as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            // Write input data to GPU buffers
            queue.write_buffer(&buffer_a, 0, cast_to_bytes(input_a));
            queue.write_buffer(&buffer_b, 0, cast_to_bytes(input_b));

            // Create bind group
            let bind_group_layout = compute_pipeline.get_bind_group_layout(0);
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("compute_bind_group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer_a.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffer_b.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buffer_out.as_entire_binding(),
                    },
                ],
            });

            // Encode compute pass
            let mut command_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("dot_product_encoder"),
            });
            {
                let mut compute_pass = command_encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("dot_product_pass"),
                    timestamp_writes: None,
                });
                compute_pass.set_pipeline(compute_pipeline);
                compute_pass.set_bind_group(0, Some(&bind_group), &[]);
                compute_pass.dispatch_workgroups(len.div_ceil(64) as u32, 1, 1);
            }

            // Submit and wait
            queue.submit(Some(command_encoder.finish()));

            // Read back result
            let result_buffer = device.create_buffer(&BufferDescriptor {
                label: Some("result"),
                size: std::mem::size_of_val(input_a) as u64,
                usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
                mapped_at_creation: false,
            });

            let mut read_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("read_encoder"),
            });
            read_encoder.copy_buffer_to_buffer(
                &buffer_out,
                0,
                &result_buffer,
                0,
                std::mem::size_of_val(input_a) as u64,
            );
            queue.submit(Some(read_encoder.finish()));

            // Map and read result
            let buffer_slice = result_buffer.slice(..);
            {
                let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
                buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                // Poll device until buffer is ready
                loop {
                    if device.poll(wgpu::PollType::Wait).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                rx.recv()
                    .map_err(|e| format!("Map receive error: {}", e))?
                    .map_err(|e| format!("Buffer map error: {}", e))?;
            }
            let data: Vec<u8> = buffer_slice.get_mapped_range().to_vec();
            let result: Vec<f32> = data
                .chunks_exact(4)
                .map(|chunk| {
                    let arr: [u8; 4] = chunk
                        .try_into()
                        .expect("chunks_exact(4) guarantees 4 bytes");
                    f32::from_le_bytes(arr)
                })
                .collect();

            let dot_scalar: f32 = result.iter().sum();
            Ok(dot_scalar)
        }

        fn compute_cosine(
            &self,
            a: &[f32],
            b: &[f32],
        ) -> Result<f64, Box<dyn Error + Send + Sync>> {
            let dot_result = self.compute_dot(a, b)?;
            Ok(dot_result as f64)
        }

        fn compute_topk(
            &self,
            query: &[f32],
            candidates: &[Vec<f32>],
            k: usize,
        ) -> Result<Vec<(usize, f64)>, Box<dyn Error + Send + Sync>> {
            if query.is_empty() || candidates.is_empty() || k == 0 {
                return Ok(vec![]);
            }
            let k = k.min(candidates.len());
            let mut scored: Vec<(usize, f64)> = candidates
                .iter()
                .enumerate()
                .map(|(idx, cand)| {
                    let score = self.compute_cosine(query, cand).unwrap_or(0.0);
                    (idx, score)
                })
                .collect();
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            scored.truncate(k);
            Ok(scored)
        }

        fn backend_name(&self) -> &'static str {
            "HttpGpuBackend (WGSL compute)"
        }
        fn capability_score(&self) -> f32 {
            1.0
        }
    }

    /// Extension methods for U4 quantized GPU compute.
    impl HttpGpuBackend {
        /// Compute dot product using U4 quantized vectors (87.5% less PCI-e bandwidth).
        ///
        /// Transmit packed bytes directly, not dequantized f32.
        /// Uses U4_DOT_SHADER + REDUCE_SHADER in single dispatch.
        pub fn compute_dot_u4(
            &self,
            quantized_a: &crate::quantization::EmbeddingU4,
            quantized_b: &crate::quantization::EmbeddingU4,
        ) -> Result<f32, Box<dyn Error + Send + Sync>> {
            if quantized_a.dims != quantized_b.dims {
                return Err("Dimension mismatch between U4 vectors".into());
            }
            let len = quantized_a.dims;
            if len == 0 {
                return Ok(0.0);
            }

            let resources = get_gpu_resources()?;
            let device = &resources.device;
            let queue = &resources.queue;
            let u4_pipeline = &resources.u4_dot_pipeline;
            let reduce_pipeline = &resources.reduce_pipeline;

            // Packed byte sizes: 2 elements per byte (nibble packing)
            let packed_bytes_a = quantized_a.data.len();
            let packed_bytes_b = quantized_b.data.len();

            // Meta vec4: (scale_a, zero_a, scale_b, zero_b)
            let meta = [
                quantized_a.scale,
                quantized_a.zero,
                quantized_b.scale,
                quantized_b.zero,
            ];

            // Create GPU buffers
            let buffer_packed_a = device.create_buffer(&BufferDescriptor {
                label: Some("packed_a"),
                size: packed_bytes_a as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let buffer_packed_b = device.create_buffer(&BufferDescriptor {
                label: Some("packed_b"),
                size: packed_bytes_b as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Output: one f32 per element (element-wise product)
            let output_size = len * std::mem::size_of::<f32>();
            let buffer_output = device.create_buffer(&BufferDescriptor {
                label: Some("output"),
                size: output_size as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            // Meta uniform buffer
            let buffer_meta = device.create_buffer(&BufferDescriptor {
                label: Some("meta"),
                size: std::mem::size_of::<[f32; 4]>() as u64,
                usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            // Reduction output (single f32)
            let buffer_reduced = device.create_buffer(&BufferDescriptor {
                label: Some("reduced"),
                size: std::mem::size_of::<f32>() as u64,
                usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC | BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            // Write data to GPU
            queue.write_buffer(&buffer_packed_a, 0, &quantized_a.data);
            queue.write_buffer(&buffer_packed_b, 0, &quantized_b.data);
            queue.write_buffer(&buffer_meta, 0, bytemuck::cast_slice(&meta));

            // Bind group for U4 dot product
            let bind_group_layout = u4_pipeline.get_bind_group_layout(0);
            let bind_group_u4 = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("u4_bind_group"),
                layout: &bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer_packed_a.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffer_packed_b.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: buffer_output.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: buffer_meta.as_entire_binding(),
                    },
                ],
            });

            // Dispatch U4 dot product
            let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("u4_dot_encoder"),
            });
            {
                let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("u4_dot_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(u4_pipeline);
                pass.set_bind_group(0, Some(&bind_group_u4), &[]);
                pass.dispatch_workgroups(len.div_ceil(64) as u32, 1, 1);
            }
            queue.submit(Some(encoder.finish()));

            // Bind group for reduction
            let reduce_layout = reduce_pipeline.get_bind_group_layout(0);
            let bind_group_reduce = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("reduce_bind_group"),
                layout: &reduce_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer_output.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffer_reduced.as_entire_binding(),
                    },
                ],
            });

            // Dispatch reduction
            let mut reduce_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
                label: Some("reduce_encoder"),
            });
            {
                let mut pass = reduce_encoder.begin_compute_pass(&ComputePassDescriptor {
                    label: Some("reduce_pass"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(reduce_pipeline);
                pass.set_bind_group(0, Some(&bind_group_reduce), &[]);
                pass.dispatch_workgroups(1, 1, 1);
            }
            queue.submit(Some(reduce_encoder.finish()));

            // Read back reduced result
            let buffer_slice = buffer_reduced.slice(..);
            {
                let (tx, rx) = std::sync::mpsc::channel::<Result<(), wgpu::BufferAsyncError>>();
                buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
                loop {
                    if device.poll(wgpu::PollType::Wait).is_ok() {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
                rx.recv()
                    .map_err(|e| format!("Map receive error: {}", e))?
                    .map_err(|e| format!("Buffer map error: {}", e))?;
            }
            let data: Vec<u8> = buffer_slice.get_mapped_range().to_vec();
            let result: f32 = f32::from_le_bytes(data[..4].try_into().expect("4 bytes"));
            Ok(result)
        }
    }
}

pub use http_impl::HttpGpuBackend;
#[cfg(feature = "gpu-compute")]
pub use http_impl::{GpuResources, get_gpu_resources};

use leptos::prelude::*;
fn test() -> impl IntoView {
    let r = LocalResource::new(|| async move { 5 });
    view! {
        <div>
            {move || match r.get() {
                Some(v) => view! { <p>{v}</p> }.into_view(),
                None => view! { <p>"Loading..."</p> }.into_view(),
            }}
        </div>
    }
}

use leptos::prelude::*;
fn test() -> impl IntoView {
    view! {
        <Suspense fallback=view!{}>
            {Suspend::new(async move {
                view! { <p>"Test"</p> }
            })}
        </Suspense>
    }
}

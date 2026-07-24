use deno_core::JsRuntime;

fn main() {
    let mut runtime = JsRuntime::new(deno_core::RuntimeOptions::default());
    let _ = runtime.execute_script("test", "1+1".into());
}

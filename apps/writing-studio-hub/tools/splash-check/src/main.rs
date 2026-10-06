//! Dry-run syntax check for a .splash body using the real makepad script VM.
//! Widgets/fs/host/net are not registered here, so runtime "not found" errors
//! are expected; only tokenize/parse errors fail the check.
use makepad_script::*;

fn main() {
    let path = std::env::args().nth(1).expect("usage: splash-check <file.splash>");
    let code = std::fs::read_to_string(&path).expect("cannot read file");
    let mut host = ScriptVmHost::new(0, ());
    let vm = &mut ScriptVm {
        host: &mut host,
        bx: Box::new(ScriptVmBase::new()),
    };
    let script_mod = ScriptMod {
        cargo_manifest_path: String::new(),
        module_path: "splash-check".to_string(),
        file: path.clone(),
        line: 0,
        column: 0,
        code: String::new(),
        values: vec![],
    };
    vm.bx.captured_errors = Some(Vec::new());
    let _ = vm.with_instruction_limit(4_000_000, |vm| {
        vm.eval_with_append_source(script_mod, &code, NIL.into())
    });
    let errors = vm.take_errors();
    let mut fatal = false;
    for e in &errors {
        let syntax = e.contains("tokeniz")
            || e.contains("parse")
            || e.contains("syntax")
            || e.contains("expected")
            || e.contains("unexpected");
        if syntax {
            fatal = true;
        }
        println!("{}: {}", if syntax { "SYNTAX" } else { "runtime(ok)" }, e);
    }
    if fatal {
        std::process::exit(1);
    }
    println!("PARSE OK: {} ({} bytes)", path, code.len());
}

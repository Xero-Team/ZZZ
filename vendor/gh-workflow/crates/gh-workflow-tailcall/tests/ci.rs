use gh_workflow_tailcall::{StandardWorkflow, generate_release_drafter, generate_release_publish};

#[test]
fn generate() {
    StandardWorkflow::default()
        .auto_fix(true)
        .to_owned()
        .generate()
        .unwrap();
}

#[test]
fn generate_release_workflows() {
    generate_release_drafter().unwrap();
    generate_release_publish().unwrap();
}

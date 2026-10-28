#[test]
fn five_scenes_against_the_mock_engine() {
    let report = dj_gateway::run_all().expect("demo scenes");
    assert!(report.contains("scene 1:"));
    assert!(report.contains("PASS"));
    assert!(report.contains("audit verify: ok"));
}

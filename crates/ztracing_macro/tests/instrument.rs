use ztracing_macro::instrument;

#[instrument]
fn increment(value: i32) -> i32 {
    value + 1
}

#[instrument]
fn pick_first<T: Copy>(left: T, _right: T) -> T {
    left
}

#[test]
fn instrument_attribute_preserves_function_behavior() {
    assert_eq!(increment(41), 42);
    assert_eq!(pick_first("left", "right"), "left");
}

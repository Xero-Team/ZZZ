// Rust Analyzer's proc-macro server can take up to ten seconds to expand this macro.
#[cfg_attr(not(rust_analyzer), gpui_macros::derive_inspector_reflection)]
trait Transform: Clone {
    /// Doubles the value
    fn double(self) -> Self;

    /// Triples the value
    fn triple(self) -> Self;

    /// Increments the value by one
    ///
    /// This method has a default implementation
    fn increment(self) -> Self {
        self.add_one()
    }

    /// Quadruples the value by doubling twice
    fn quadruple(self) -> Self {
        self.double().double()
    }

    // These methods will be filtered out:
    #[allow(dead_code)]
    fn add(&self, other: &Self) -> Self;
    #[allow(dead_code)]
    fn set_value(&mut self, value: i32);
    #[allow(dead_code)]
    fn get_value(&self) -> i32;

    /// Adds one to the value
    fn add_one(self) -> Self;
}

#[derive(Debug, Clone, PartialEq)]
struct Number(i32);

impl Transform for Number {
    fn double(self) -> Self {
        Number(self.0 * 2)
    }

    fn triple(self) -> Self {
        Number(self.0 * 3)
    }

    fn add(&self, other: &Self) -> Self {
        Number(self.0 + other.0)
    }

    fn set_value(&mut self, value: i32) {
        self.0 = value;
    }

    fn get_value(&self) -> i32 {
        self.0
    }

    fn add_one(self) -> Self {
        Number(self.0 + 1)
    }
}

#[test]
fn test_derive_inspector_reflection() {
    use transform_reflection::*;

    let methods = methods::<Number>();

    assert_eq!(methods.len(), 5);
    let method_names: Vec<_> = methods.iter().map(|method| method.name).collect();
    assert!(method_names.contains(&"double"));
    assert!(method_names.contains(&"triple"));
    assert!(method_names.contains(&"increment"));
    assert!(method_names.contains(&"quadruple"));
    assert!(method_names.contains(&"add_one"));

    let number = Number(5);

    let doubled = find_method::<Number>("double")
        .expect("double should be reflected")
        .invoke(number.clone());
    assert_eq!(doubled, Number(10));

    let tripled = find_method::<Number>("triple")
        .expect("triple should be reflected")
        .invoke(number.clone());
    assert_eq!(tripled, Number(15));

    let incremented = find_method::<Number>("increment")
        .expect("increment should be reflected")
        .invoke(number.clone());
    assert_eq!(incremented, Number(6));

    let quadrupled = find_method::<Number>("quadruple")
        .expect("quadruple should be reflected")
        .invoke(number);
    assert_eq!(quadrupled, Number(20));

    let result = find_method::<Number>("nonexistent");
    assert!(result.is_none());

    let number = Number(10);
    let result = find_method::<Number>("double")
        .map(|method| method.invoke(number))
        .and_then(|number| find_method::<Number>("increment").map(|method| method.invoke(number)))
        .and_then(|number| find_method::<Number>("triple").map(|method| method.invoke(number)));

    assert_eq!(result, Some(Number(63)));

    let double_method = find_method::<Number>("double").expect("double should be reflected");
    assert_eq!(double_method.documentation, Some("Doubles the value"));

    let triple_method = find_method::<Number>("triple").expect("triple should be reflected");
    assert_eq!(triple_method.documentation, Some("Triples the value"));

    let increment_method =
        find_method::<Number>("increment").expect("increment should be reflected");
    assert_eq!(
        increment_method.documentation,
        Some("Increments the value by one\n\nThis method has a default implementation")
    );

    let quadruple_method =
        find_method::<Number>("quadruple").expect("quadruple should be reflected");
    assert_eq!(
        quadruple_method.documentation,
        Some("Quadruples the value by doubling twice")
    );

    let add_one_method = find_method::<Number>("add_one").expect("add_one should be reflected");
    assert_eq!(add_one_method.documentation, Some("Adds one to the value"));
}

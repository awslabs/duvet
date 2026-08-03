// Corpus for the duvet Rust-classifier macro-policy question.
// Each construct is line-number-annotated in the evidence writeup.
// Functions with `covered_` prefix are called from the test; `uncovered_`
// are compiled but never called, so LCOV should report Miss (DA:n,0).

// --- 1. multi-line vec! -------------------------------------------------
pub fn covered_vec() -> Vec<i32> {
    let v = vec![
        1,
        2,
        3,
    ];
    v
}

pub fn uncovered_vec() -> Vec<i32> {
    let v = vec![
        10,
        20,
        30,
    ];
    v
}

// --- 2. multi-line println! ---------------------------------------------
pub fn covered_println(x: i32) {
    println!(
        "value is {} and doubled is {}",
        x,
        x * 2,
    );
}

pub fn uncovered_println(x: i32) {
    println!(
        "never printed {} {}",
        x,
        x + 1,
    );
}

// --- 3. custom declarative macro ------------------------------------------
macro_rules! accumulate {
    ($($x:expr),* $(,)?) => {{
        let mut total = 0;
        $(
            total += $x;
        )*
        total
    }};
}

pub fn covered_custom_macro() -> i32 {
    let total = accumulate![
        1,
        2,
        3,
    ];
    total
}

pub fn uncovered_custom_macro() -> i32 {
    let total = accumulate![
        7,
        8,
    ];
    total
}

// --- 4. multi-line matches! -----------------------------------------------
pub fn covered_matches(x: Option<i32>) -> bool {
    matches!(
        x,
        Some(n) if n > 0
    )
}

pub fn uncovered_matches(x: Option<i32>) -> bool {
    matches!(
        x,
        None
    )
}

// --- 5. assert! variants ---------------------------------------------------
pub fn covered_asserts(x: i32) -> i32 {
    assert!(
        x > 0,
        "x must be positive, got {}",
        x,
    );
    assert_eq!(
        x + 0,
        x,
    );
    debug_assert!(
        x < 1_000_000,
    );
    x
}

pub fn uncovered_asserts(x: i32) -> i32 {
    assert_ne!(
        x,
        -1,
    );
    x
}

// --- 6. cfg-gated code -------------------------------------------------------
#[cfg(feature = "never-enabled")]
pub fn cfg_gated_out() -> i32 {
    let a = vec![
        100,
        200,
    ];
    a.len() as i32
}

#[cfg(not(feature = "never-enabled"))]
pub fn cfg_gated_in() -> i32 {
    42
}

// --- 7. #[test] module --------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exercise_covered_half() {
        assert_eq!(covered_vec(), vec![1, 2, 3]);
        covered_println(5);
        assert_eq!(covered_custom_macro(), 6);
        assert!(covered_matches(Some(3)));
        assert_eq!(covered_asserts(7), 7);
        assert_eq!(cfg_gated_in(), 42);
    }

    #[test]
    fn never_run_test() {
        // Present but filtered out at runtime (see run command); its body
        // shows what LCOV reports for a compiled-but-not-run #[test].
        assert_eq!(uncovered_vec(), vec![10, 20, 30]);
    }
}

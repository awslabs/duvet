// Two macros. At the call site they look IDENTICAL.
// pick_first throws the second argument away. add_both keeps both.

macro_rules! pick_first {
    ($a:expr, $b:expr $(,)?) => { $a };
}

macro_rules! add_both {
    ($a:expr, $b:expr $(,)?) => { $a + $b };
}

pub fn use_pick(x: i32, y: i32) -> i32 {
    pick_first!(
        x * 2,
        y * 3,
    )
}

pub fn use_add(x: i32, y: i32) -> i32 {
    add_both!(
        x * 2,
        y * 3,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn run_both() {
        assert_eq!(use_pick(1, 1), 2);
        assert_eq!(use_add(1, 1), 5);
    }
}

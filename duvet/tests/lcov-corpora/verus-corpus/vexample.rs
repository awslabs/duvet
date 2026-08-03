// Representative verus! block for the duvet Rust-classifier evidence package.
use vstd::prelude::*;

verus! {

// spec fn: ghost-only, erased before codegen
spec fn abs_spec(x: int) -> int {
    if x >= 0 {
        x
    } else {
        -x
    }
}

// proof fn: ghost-only, erased before codegen
proof fn abs_nonneg(x: int)
    ensures
        abs_spec(x) >= 0,
{
    assert(abs_spec(x) >= 0);
}

// exec fn: real code, survives erasure
fn abs_exec(x: i64) -> (r: i64)
    requires
        x != i64::MIN,
    ensures
        r == abs_spec(x as int),
        r >= 0,
{
    proof {
        abs_nonneg(x as int);
    }
    if x >= 0 {
        x
    } else {
        -x
    }
}

fn uncalled_exec(x: i64) -> (r: i64)
    requires
        x < 1000,
    ensures
        r == x + 1,
{
    proof {
        abs_nonneg(x as int);
    }
    x + 1
}

assume_specification [crate::print_results] (a: i64, b: i64);

fn main() {
    let a = abs_exec(-5);
    let b = abs_exec(7);
    proof {
        abs_nonneg(7int);
    }
    print_results(a, b);
}

} // verus!

// Ordinary Rust outside the verus! block — Verus does not restrict it.
fn print_results(a: i64, b: i64) {
    println!(
        "abs results: {} {}",
        a,
        b,
    );
}

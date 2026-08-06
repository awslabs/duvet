package corpus

// Single-expression functions: `fun f() = expr`. No braces, no block body.
// Question: which line carries the JaCoCo probe? The declaration line, or the
// expression line when the expression is on a following line?

fun exprCalled(x: Int) = x * 2

fun exprNotCalled(x: Int) = x * 3

// Expression on its own line (declaration and expression on different lines)
fun exprSplitCalled(x: Int) =
    x + 100

fun exprSplitNotCalled(x: Int) =
    x + 200

// Multi-line expression body (chain spanning several lines)
fun exprChainCalled(xs: List<Int>) =
    xs.map { it + 1 }
        .filter { it > 0 }
        .sum()

fun runSingleExpr() {
    println(exprCalled(1))
    println(exprSplitCalled(1))
    println(exprChainCalled(listOf(1, 2, 3)))
}

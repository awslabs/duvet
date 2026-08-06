package corpus

// Labeled jumps: label declaration (`outer@`), break@label, continue@label,
// return@label from lambdas. The NonLinearControl inventory candidates.

fun labeledBreak(): Int {
    var total = 0
    outer@ for (i in 1..3) {
        for (j in 1..3) {
            if (i * j > 4) {
                break@outer
            }
            total += i * j
        }
    }
    return total
}

fun labeledContinue(): Int {
    var total = 0
    loop@ for (i in 1..3) {
        for (j in 1..3) {
            if (j == 2) {
                continue@loop
            }
            total += j
        }
    }
    return total
}

fun labeledReturnFromLambda(xs: List<Int>): Int {
    var count = 0
    xs.forEach lit@{ n ->
        if (n == 2) {
            return@lit
        }
        count += 1
    }
    return count
}

// Implicit label: return@forEach
fun implicitLabelReturn(xs: List<Int>): Int {
    var count = 0
    xs.forEach {
        if (it < 0) {
            return@forEach
        }
        count += 1
    }
    return count
}

fun runLabels() {
    println(labeledBreak())
    println(labeledContinue())
    println(labeledReturnFromLambda(listOf(1, 2, 3)))
    println(implicitLabelReturn(listOf(1, -1, 2)))
}

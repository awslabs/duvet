package corpus

// Try/catch/finally (expression form and statement form), throw, while,
// do-while, bare break/continue. Question: does `} catch (...) {` probe like
// Java's catch_clause header, and does try-as-expression change placement?

fun tryStatement(n: Int): Int {
    var r = 0
    try {
        r = 100 / n
    } catch (e: ArithmeticException) {
        r = -1
    } finally {
        r += 1
    }
    return r
}

fun tryExpression(n: Int): Int {
    val v = try {
        100 / n
    } catch (e: ArithmeticException) {
        -1
    }
    return v
}

fun throwNotCaught(flag: Boolean): Int {
    if (flag) {
        throw IllegalStateException("boom")
    }
    return 7
}

fun whileLoop(n: Int): Int {
    var i = 0
    var acc = 0
    while (i < n) {
        acc += i
        i += 1
    }
    return acc
}

fun doWhileLoop(n: Int): Int {
    var i = 0
    var acc = 0
    do {
        acc += 2
        i += 1
    } while (i < n)
    return acc
}

fun bareBreakContinue(n: Int): Int {
    var acc = 0
    for (i in 1..n) {
        if (i == 2) {
            continue
        }
        if (i > 4) {
            break
        }
        acc += i
    }
    return acc
}

fun runTryCatch() {
    println(tryStatement(4))
    println(tryStatement(0))
    println(tryExpression(5))
    println(tryExpression(0))
    println(throwNotCaught(false))
    try {
        throwNotCaught(true)
    } catch (e: IllegalStateException) {
        println("caught")
    }
    println(whileLoop(4))
    println(doWhileLoop(3))
    println(bareBreakContinue(6))
}

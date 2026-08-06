package corpus

// Lambdas and trailing lambdas. Question: does the lambda body get its own
// probes (it compiles to a separate method / invokedynamic), and which lines?

fun runLambdas() {
    val doubler = { x: Int ->
        x * 2
    }
    println(doubler(21))

    // Lambda assigned but never invoked: creation executes, body does not.
    val neverRun = { x: Int ->
        x * 999
    }
    println(neverRun.hashCode())

    // Trailing lambda, executed per element
    val out = listOf(1, 2, 3).map { n ->
        n + 10
    }
    println(out)

    // Trailing lambda on a collection that is empty: body never executes
    val none = emptyList<Int>().map { n ->
        n * 7
    }
    println(none)

    // Multi-statement trailing lambda
    listOf(4, 5).forEach { n ->
        val m = n * 3
        println(m)
    }
}

package corpus

// `when` expressions: subject form, subjectless form, expression vs statement,
// block-bodied arms. Question: per-arm probes (like Java switch arrows)?

fun whenSubject(n: Int): String {
    return when (n) {
        0 -> "zero"
        1 -> "one"
        else -> "many"
    }
}

fun whenSubjectless(n: Int): String {
    return when {
        n < 0 -> "negative"
        n == 0 -> "zero"
        else -> "positive"
    }
}

fun whenBlockArms(n: Int): String {
    return when (n) {
        0 -> {
            val s = "z" + "ero"
            s
        }
        else -> {
            val s = "ma" + "ny"
            s
        }
    }
}

// A when expression used as a single-expression function body
fun whenExprBody(n: Int) = when (n) {
    0 -> "zero"
    else -> "nonzero"
}

fun runWhen() {
    // exercise: subject form hits 0 arm only; subjectless hits positive arm only;
    // block form hits else arm only; expr-body form hits 0 arm only.
    println(whenSubject(0))
    println(whenSubjectless(5))
    println(whenBlockArms(3))
    println(whenExprBody(0))
}

package corpus

// Inline functions: bodies are inlined into call sites. Question: do the
// inline function's own declaration lines get probes, are they attributed to
// the caller, or both? Also: noinline/crossinline lambdas.

inline fun inlineCalled(block: () -> Int): Int {
    val before = 1
    val result = block()
    return before + result
}

inline fun inlineNotCalled(block: () -> Int): Int {
    val hidden = 5
    return hidden + block()
}

// Inline with a reified type parameter (forces inlining)
inline fun <reified T> typeName(): String {
    return T::class.simpleName ?: "?"
}

fun runInline() {
    val r = inlineCalled {
        41
    }
    println(r)
    println(typeName<String>())
}

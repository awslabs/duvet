package corpus

// Default arguments: the compiler emits a synthetic `$default` bridge.
// Question: does calling WITH all args vs relying on defaults change which
// lines are hit — and do default-value expressions carry their own probes?

fun greet(
    name: String = "world",
    punct: String = "!",
    times: Int = 1
): String {
    var out = ""
    repeat(times) {
        out += "hello $name$punct"
    }
    return out
}

// Called only with all arguments — the default expressions never evaluate
fun allArgsProvided(a: Int = 10, b: Int = 20): Int {
    return a + b
}

// Never called at all
fun defaultsNeverCalled(z: Int = 99): Int {
    return z + 1
}

fun runDefaults() {
    println(greet())                    // all defaults evaluate
    println(greet("kotlin", ".", 2))    // no defaults evaluate
    println(allArgsProvided(1, 2))      // defaults never evaluate
}

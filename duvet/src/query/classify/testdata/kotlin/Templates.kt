package corpus

// Multi-line string templates (raw strings with interpolation).
// Question: which lines of a multi-line template carry probes — the opening
// line, each interpolated line, or the whole span?

fun templateCalled(name: String, n: Int): String {
    val s = """
        hello $name
        you have ${n + 1} messages
        plain line no interpolation
        computed: ${
            n * 2
        }
        bye
    """.trimIndent()
    return s
}

fun templateNotCalled(name: String): String {
    val s = """
        never $name
        ${name.length + 42}
    """.trimIndent()
    return s
}

fun runTemplates() {
    println(templateCalled("world", 2))
}

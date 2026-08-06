package corpus

// Data classes: the compiler synthesizes equals/hashCode/toString/copy/componentN.
// Question: where do the synthesized members' probes land? (Expected: the
// data class declaration line, like Java records attribute to the header.)

data class UsedFully(val x: Int, val y: String)

data class UsedPartially(val a: Int, val b: Int)

data class NeverInstantiated(val q: Int)

// Data class with a body: real methods inside, synthesized ones on the header?
data class WithBody(val v: Int) {
    fun doubled(): Int {
        return v * 2
    }

    fun neverCalledMethod(): Int {
        return v * 3
    }
}

fun runDataClasses() {
    val u1 = UsedFully(1, "a")
    val u2 = u1.copy(x = 2)
    println(u1 == u2)
    println(u1.hashCode())
    println(u1.toString())
    val (x, y) = u1
    println("$x$y")

    // Only construct; never call equals/hashCode/copy
    val p = UsedPartially(1, 2)
    println(p.a)

    val w = WithBody(21)
    println(w.doubled())
}

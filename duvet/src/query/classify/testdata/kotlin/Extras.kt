package corpus

// Extras that shape the classifier's node-kind mapping: init blocks, custom
// property accessors, companion objects, object declarations, top-level
// property initializers, if-expressions, elvis/safe-call chains.

val topLevelInitialized: Int = 7 * 6

val topLevelLazy: String by lazy {
    "computed" + "!"
}

class WithInit(seed: Int) {
    val stored: Int

    init {
        stored = seed * 2
    }

    val computed: Int
        get() {
            return stored + 1
        }

    var settable: Int = 0
        set(value) {
            field = value * 10
        }

    companion object {
        fun fromDefault(): WithInit {
            return WithInit(5)
        }

        fun neverUsedFactory(): WithInit {
            return WithInit(99)
        }
    }
}

object Singleton {
    val name = "single"

    fun used(): String {
        return name + "ton"
    }

    fun notUsed(): String {
        return name + "!!"
    }
}

fun ifExpression(n: Int): Int {
    val v = if (n > 0) {
        n * 2
    } else {
        n * -2
    }
    return v
}

fun elvisChain(s: String?): Int {
    val len = s?.length ?: -1
    return len
}

fun runExtras() {
    println(topLevelInitialized)
    println(topLevelLazy)
    val w = WithInit.fromDefault()
    println(w.computed)
    w.settable = 3
    println(w.settable)
    println(Singleton.used())
    println(ifExpression(4))
    println(elvisChain(null))
    println(elvisChain("abc"))
}

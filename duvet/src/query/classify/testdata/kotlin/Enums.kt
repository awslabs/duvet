package corpus

// Enum classes (with ctor args + methods), sealed hierarchies, interfaces with
// default methods, secondary constructors, typealias, annotation use-site.
// Question: enum entries — do they probe like Java enum constants (on the
// entry line)? Where do sealed subclass headers and secondary ctors probe?

enum class Color(val rgb: Int) {
    RED(0xFF0000),
    GREEN(0x00FF00),
    BLUE(0x0000FF);

    fun hex(): String {
        return "#%06X".format(rgb)
    }

    fun neverCalled(): Int {
        return rgb * 2
    }
}

enum class Plain {
    A,
    B,
}

sealed class Shape {
    class Circle(val r: Double) : Shape()
    class Square(val side: Double) : Shape()
    object Unit_ : Shape()
}

fun area(s: Shape): Double = when (s) {
    is Shape.Circle -> 3.14 * s.r * s.r
    is Shape.Square -> s.side * s.side
    Shape.Unit_ -> 1.0
}

interface Greeter {
    fun name(): String

    fun greet(): String {
        return "hi " + name()
    }
}

class NamedGreeter(private val n: String) : Greeter {
    // secondary constructor
    constructor() : this("anon") {
        println("secondary ran")
    }

    override fun name(): String {
        return n
    }
}

typealias IntPair = Pair<Int, Int>

@Deprecated("use runEnums")
fun oldEntry(): Int {
    return 1
}

fun runEnums() {
    println(Color.RED.hex())
    println(Plain.A)
    println(area(Shape.Circle(1.0)))
    println(area(Shape.Unit_))
    println(NamedGreeter("kim").greet())
    println(NamedGreeter().greet())
    val p: IntPair = Pair(1, 2)
    println(p.first)
    println(oldEntry())
}

package corpus

import kotlin.coroutines.Continuation
import kotlin.coroutines.EmptyCoroutineContext
import kotlin.coroutines.startCoroutine
import kotlin.coroutines.resume
import kotlin.coroutines.suspendCoroutine

// Suspend functions: the coroutine state-machine transform rewrites the body
// into a switch over states. Question: how does that transform distort line
// probes — extra branches on suspension-point lines, probes on the fun header?

suspend fun suspendCalled(x: Int): Int {
    val a = x + 1
    val b = pause(a)
    val c = b + 1
    return c
}

// A real suspension point that resumes immediately
suspend fun pause(v: Int): Int = suspendCoroutine { cont ->
    cont.resume(v * 2)
}

suspend fun suspendNotCalled(x: Int): Int {
    val dead = x * 5
    return pause(dead)
}

// Suspend with multiple suspension points
suspend fun multiSuspend(x: Int): Int {
    val first = pause(x)
    val second = pause(first)
    return second + 1
}

fun runSuspend() {
    // Drive without kotlinx-coroutines: startCoroutine + empty completion
    val done = java.util.concurrent.CountDownLatch(1)
    val body: suspend () -> Int = {
        val r1 = suspendCalled(10)
        val r2 = multiSuspend(r1)
        r2
    }
    body.startCoroutine(object : Continuation<Int> {
        override val context = EmptyCoroutineContext
        override fun resumeWith(result: Result<Int>) {
            println(result.getOrThrow())
            done.countDown()
        }
    })
    done.await()
}

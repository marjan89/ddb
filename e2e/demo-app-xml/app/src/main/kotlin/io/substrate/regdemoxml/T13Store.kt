package io.substrate.regdemo.xml

enum class T13State { Locked, Unlocked, Error }

object T13Store {
    @Volatile var state: T13State = T13State.Locked
        private set

    private val listeners = mutableListOf<(T13State) -> Unit>()

    fun observe(l: (T13State) -> Unit) {
        listeners.add(l)
        l(state)
    }

    fun unobserve(l: (T13State) -> Unit) {
        listeners.remove(l)
    }

    private fun set(s: T13State) {
        state = s
        listeners.toList().forEach { it(s) }
    }

    fun handle(email: String, password: String, completion: (Boolean, String?) -> Unit) {
        if (email == "t13@example.com" && password == "t13pass") {
            set(T13State.Unlocked)
            completion(true, null)
        } else {
            set(T13State.Error)
            completion(false, "Invalid credentials")
        }
    }

    fun reset() { set(T13State.Locked) }
}

package io.substrate.regdemo

import androidx.compose.runtime.mutableStateOf

enum class T13State { Locked, Unlocked, Error }

object T13Store {
    val state = mutableStateOf(T13State.Locked)

    fun handle(email: String, password: String, completion: (Boolean, String?) -> Unit) {
        if (email == "t13@example.com" && password == "t13pass") {
            state.value = T13State.Unlocked
            completion(true, null)
        } else {
            state.value = T13State.Error
            completion(false, "Invalid credentials")
        }
    }

    fun reset() {
        state.value = T13State.Locked
    }
}

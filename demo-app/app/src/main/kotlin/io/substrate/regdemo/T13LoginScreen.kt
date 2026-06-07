package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import dev.substrate.semantic.SemanticAgent

private enum class T13State { Locked, Unlocked, Error }

@Composable
fun T13LoginScreen() {
    var state by remember { mutableStateOf(T13State.Locked) }
    DisposableEffect(Unit) {
        SemanticAgent.loginHandler = { email, password, completion ->
            if (email == "t13@example.com" && password == "t13pass") {
                state = T13State.Unlocked
                completion(true, null)
            } else {
                state = T13State.Error
                completion(false, "Invalid credentials")
            }
        }
        onDispose { SemanticAgent.loginHandler = null }
    }
    val primary = when (state) {
        T13State.Locked, T13State.Error -> "T13 Locked"
        T13State.Unlocked -> "T13 Unlocked"
    }
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T13 Login")
        Text(primary, modifier = Modifier.semantics { contentDescription = primary })
        if (state == T13State.Error) {
            Text("Invalid credentials", modifier = Modifier.semantics { contentDescription = "Invalid credentials" })
        }
        Button(onClick = { state = T13State.Locked }) {
            Text("T13 Reset")
        }
    }
}

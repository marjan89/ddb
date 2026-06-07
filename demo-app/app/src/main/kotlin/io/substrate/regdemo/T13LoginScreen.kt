package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
fun T13LoginScreen() {
    val state by T13Store.state
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
        Button(onClick = { T13Store.reset() }) {
            Text("T13 Reset")
        }
    }
}

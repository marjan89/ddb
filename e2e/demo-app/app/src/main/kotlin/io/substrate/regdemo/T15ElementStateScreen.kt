package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
fun T15ElementStateScreen() {
    var disabled by remember { mutableStateOf(false) }
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        var actionTaps by remember { mutableStateOf(0) }
        Text("T15 State")
        Button(onClick = { actionTaps++ }, enabled = !disabled) {
            Text("Action Button")
        }
        Text(
            "taps=$actionTaps",
            modifier = Modifier.semantics { contentDescription = "taps=$actionTaps" },
        )
        Button(onClick = { disabled = !disabled }) {
            Text("Toggle Action")
        }
        Text(
            if (disabled) "T15 disabled state" else "T15 enabled state",
            modifier = Modifier.semantics {
                contentDescription = if (disabled) "T15 disabled state" else "T15 enabled state"
            },
        )
    }
}

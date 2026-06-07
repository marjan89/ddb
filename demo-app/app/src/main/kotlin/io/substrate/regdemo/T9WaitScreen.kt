package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay

@Composable
fun T9WaitScreen() {
    // TD-56 workaround: always-mounted Text whose text-state toggles. Conditional
    // mount (`if (revealed) Text(...)`) does not surface via the walker's
    // AccessibilityNodeInfo provider until something forces an a11y refresh.
    // A composable that is always present and only changes its String state
    // recomposes its existing semantic node, which IS picked up reliably.
    var label by remember { mutableStateOf("") }
    var trigger by remember { mutableIntStateOf(0) }
    LaunchedEffect(trigger) {
        if (trigger > 0) {
            label = ""
            delay(3000)
            label = "T9 Delayed Element"
        }
    }
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T9 Wait")
        Button(onClick = { trigger++ }) {
            Text("T9 Trigger")
        }
        Text(
            label,
            modifier = Modifier.semantics { contentDescription = label.ifEmpty { "t9-placeholder" } },
        )
    }
}

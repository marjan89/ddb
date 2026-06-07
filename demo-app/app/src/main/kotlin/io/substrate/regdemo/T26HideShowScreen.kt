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
fun T26HideShowScreen() {
    var visible by remember { mutableStateOf(true) }
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T26 Hide Show")
        // Always-mount Text with toggling content (R19 Android sibling pattern;
        // avoids late-mount a11y race when toggling visibility).
        val label = if (visible) "T26 Target" else ""
        Text(label, modifier = Modifier.semantics { contentDescription = label.ifEmpty { "t26-hidden" } })
        Button(onClick = { visible = !visible }) {
            Text("T26 Toggle Visibility")
        }
    }
}

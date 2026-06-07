package io.substrate.regdemo

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
fun T22LongPressScreen() {
    var pressed by remember { mutableStateOf(false) }
    val label = if (pressed) "T22 Pressed" else "T22 Idle"
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(24.dp),
    ) {
        Text("T22 Long Press")
        Text(
            "T22 Press Target",
            modifier = Modifier
                .background(Color.LightGray)
                .padding(16.dp)
                .semantics { contentDescription = "T22 Press Target" }
                .pointerInput(Unit) {
                    detectTapGestures(onLongPress = { pressed = !pressed })
                },
        )
        Text(label, modifier = Modifier.semantics { contentDescription = label })
    }
}

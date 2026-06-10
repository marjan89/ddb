package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.material3.TextField
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
fun T12PasteScreen() {
    var fieldText by remember { mutableStateOf("") }
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T12 Paste")
        TextField(
            value = fieldText,
            onValueChange = { fieldText = it },
            placeholder = { Text("T12 Input") },
            modifier = Modifier
                .fillMaxWidth()
                .semantics { contentDescription = "T12 Input" },
        )
        Text(
            "T12 Value: $fieldText",
            modifier = Modifier.semantics { contentDescription = "T12 Value: $fieldText" },
        )
    }
}

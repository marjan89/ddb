package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp

@Composable
fun T8ScrollScreen() {
    Column(
        modifier = Modifier
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text("T8 Scroll")
        Text("scroll.top", modifier = Modifier.semantics { contentDescription = "scroll.top" })
        for (i in 1..23) {
            Text("Row $i", modifier = Modifier.semantics { contentDescription = "scroll.row.$i" })
        }
        Text("scroll.bottom", modifier = Modifier.semantics { contentDescription = "scroll.bottom" })
    }
}

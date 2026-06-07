package io.substrate.regdemo

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun T1LaunchScreen() {
    Column(modifier = Modifier.padding(16.dp)) {
        Text("T1 Launch")
        Text("Launched")
    }
}

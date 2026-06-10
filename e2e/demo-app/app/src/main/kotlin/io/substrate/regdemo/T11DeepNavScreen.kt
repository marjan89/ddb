package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.navigation.NavController

@Composable
fun T11DeepNavScreen(nav: NavController) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("T11 Deep Nav")
        Button(onClick = { nav.navigate("t11-l1") }) { Text("T11 Level 1") }
    }
}

@Composable
fun T11Level1Screen(nav: NavController) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("T11 Level 1")
        Button(onClick = { nav.navigate("t11-l2") }) { Text("T11 Level 2") }
    }
}

@Composable
fun T11Level2Screen() {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("T11 Level 2")
        Text("T11 Deep")
    }
}

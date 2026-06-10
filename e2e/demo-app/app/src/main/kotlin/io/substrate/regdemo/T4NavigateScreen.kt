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
fun T4NavigateScreen(nav: NavController) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("T4 Navigate")
        Button(onClick = { nav.navigate("t4-detail") }) {
            Text("Go Detail")
        }
    }
}

@Composable
fun T4DetailScreen(nav: NavController) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text("T4 Detail")
        Text("Detail Visible")
        Button(onClick = { nav.popBackStack() }) {
            Text("Go Back")
        }
    }
}

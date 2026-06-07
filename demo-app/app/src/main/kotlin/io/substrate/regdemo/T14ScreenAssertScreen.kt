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
fun T14ScreenAssertScreen(nav: NavController) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T14 Screen")
        Button(onClick = { nav.navigate("t14-anchor") }) {
            Text("Open T14 Anchor")
        }
    }
}

@Composable
fun T14AnchorScreen(nav: NavController) {
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T14 Screen Anchor")
        Button(onClick = { nav.popBackStack() }) {
            Text("T14 Back")
        }
    }
}

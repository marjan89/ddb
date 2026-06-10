package io.substrate.regdemo

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@Composable
fun T18TabsScreen() {
    var selected by remember { mutableIntStateOf(0) }
    val tabs = listOf(
        Triple("T18 Goto Alpha", "T18 Tab Alpha Content", 0),
        Triple("T18 Goto Beta", "T18 Tab Beta Content", 1),
        Triple("T18 Goto Gamma", "T18 Tab Gamma Content", 2),
    )
    Scaffold(
        bottomBar = {
            NavigationBar {
                tabs.forEach { (label, _, idx) ->
                    NavigationBarItem(
                        selected = selected == idx,
                        onClick = { selected = idx },
                        icon = {},
                        label = { Text(label) },
                    )
                }
            }
        },
    ) { inner ->
        Column(modifier = Modifier.fillMaxSize().padding(inner).padding(16.dp)) {
            Text("T18 Tabs")
            Text(tabs[selected].second)
        }
    }
}

package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.navigation.NavController

@Composable
fun HomeScreen(nav: NavController) {
    val routes = listOf(
        "t1" to "T1 Launch",
        "t2" to "T2 Type",
        "t3" to "T3 Tap",
        "t4" to "T4 Navigate",
        "t5" to "T5 Kbd Dismiss",
        "t6" to "T6 Secure Field",
        "t7" to "T7 Dialog",
        "t8" to "T8 Scroll",
        "t9" to "T9 Wait",
        "t11" to "T11 Deep Nav",
        "t12" to "T12 Paste",
        "t14" to "T14 Screen",
        "t15" to "T15 State",
        "t16" to "T16 Text Equal",
        "t17" to "T17 Animations",
        "t18" to "T18 Tabs",
        "t19" to "T19 Sheet",
        "t20" to "T20 Refresh",
        "t21" to "T21 Press Back",
        "t22" to "T22 Long Press",
        "t23" to "T23 Capture",
        "t24" to "T24 Screenshot",
        "t25" to "T25 Scroll",
        "t26" to "T26 Hide Show",
        "t10" to "T10 Mock URL",
        "t13" to "T13 Login",
    )
    LazyVerticalGrid(
        columns = GridCells.Fixed(2),
        modifier = Modifier.fillMaxSize().padding(8.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        items(routes) { (route, label) ->
            Button(onClick = { nav.navigate(route) }, modifier = Modifier.fillMaxWidth()) {
                Text(label)
            }
        }
    }
}

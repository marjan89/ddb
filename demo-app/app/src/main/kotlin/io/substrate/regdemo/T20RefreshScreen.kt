package io.substrate.regdemo

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Text
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import androidx.compose.runtime.rememberCoroutineScope

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun T20RefreshScreen() {
    var counter by remember { mutableIntStateOf(0) }
    var refreshing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    PullToRefreshBox(
        modifier = Modifier.fillMaxSize(),
        isRefreshing = refreshing,
        onRefresh = {
            scope.launch {
                refreshing = true
                counter++
                delay(400)
                refreshing = false
            }
        },
    ) {
        Column(modifier = Modifier.fillMaxSize().padding(16.dp)) {
            Text("T20 Refresh")
            Text(
                "T20 Counter $counter",
                modifier = Modifier.semantics { contentDescription = "T20 Counter $counter" },
            )
        }
    }
}

package io.substrate.regdemo

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun T19SheetScreen() {
    var showSheet by remember { mutableStateOf(false) }
    val sheetState = rememberModalBottomSheetState()
    Column(
        modifier = Modifier.padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("T19 Sheet")
        Button(onClick = { showSheet = true }) {
            Text("T19 Show Sheet")
        }
    }
    if (showSheet) {
        ModalBottomSheet(
            onDismissRequest = { showSheet = false },
            sheetState = sheetState,
        ) {
            Column(
                modifier = Modifier.padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text("T19 Sheet Content")
                Button(onClick = { showSheet = false }) {
                    Text("T19 Dismiss Sheet")
                }
            }
        }
    }
}

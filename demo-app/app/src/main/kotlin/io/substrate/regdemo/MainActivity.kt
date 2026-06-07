package io.substrate.regdemo

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            MaterialTheme {
                Surface {
                    val nav = rememberNavController()
                    NavHost(navController = nav, startDestination = "home") {
                        composable("home") { HomeScreen(nav) }
                        composable("t1") { T1LaunchScreen() }
                        composable("t2") { T2TypeScreen() }
                        composable("t3") { T3TapScreen() }
                        composable("t4") { T4NavigateScreen(nav) }
                        composable("t4-detail") { T4DetailScreen(nav) }
                        composable("t5") { T5KeyboardDismissScreen() }
                        composable("t6") { T6SecureFieldScreen() }
                        composable("t7") { T7DialogScreen() }
                        composable("t8") { T8ScrollScreen() }
                        composable("t9") { T9WaitScreen() }
                        composable("t11") { T11DeepNavScreen(nav) }
                        composable("t11-l1") { T11Level1Screen(nav) }
                        composable("t11-l2") { T11Level2Screen() }
                        composable("t12") { T12PasteScreen() }
                        composable("t14") { T14ScreenAssertScreen(nav) }
                        composable("t14-anchor") { T14AnchorScreen(nav) }
                        composable("t15") { T15ElementStateScreen() }
                        composable("t16") { T16TextEqualScreen() }
                        composable("t17") { T17AnimationsScreen() }
                        composable("t18") { T18TabsScreen() }
                        composable("t19") { T19SheetScreen() }
                        composable("t20") { T20RefreshScreen() }
                        composable("t21") { T21PressBackScreen() }
                        composable("t22") { T22LongPressScreen() }
                        composable("t23") { T23CaptureScreen() }
                        composable("t24") { T24ScreenshotScreen() }
                        composable("t25") { T25ScrollScreen() }
                        composable("t26") { T26HideShowScreen() }
                        composable("t10") { T10MockURLScreen() }
                        composable("t13") { T13LoginScreen() }
                    }
                }
            }
        }
    }
}

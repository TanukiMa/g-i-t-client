# Add project specific ProGuard rules here.
# You can control the set of applied configuration files using the
# proguardFiles setting in build.gradle.
#
# For more details, see
#   http://developer.android.com/guide/developing/tools/proguard.html

# If your project uses WebView with JS, uncomment the following
# and specify the fully qualified class name to the JavaScript interface
# class:
#-keepclassmembers class fqcn.of.javascript.interface.for.webview {
#   public *;
#}

# Uncomment this to preserve the line number information for
# debugging stack traces.
#-keepattributes SourceFile,LineNumberTable

# If you keep the line number information, uncomment this to
# hide the original source file name.
#-renamesourcefileattribute SourceFile

# JNI: the Rust library finds these methods by name
-keep class jp.git_app.client.BackgroundCheck { *; }

# The page calls these methods by name (window.GiiitAndroid)
-keepclassmembers class jp.git_app.client.BillingBridge {
    @android.webkit.JavascriptInterface <methods>;
}
-keepclassmembers class jp.git_app.client.DeviceBridge {
    @android.webkit.JavascriptInterface <methods>;
}

# WorkManager makes its Room database by reflection; a release build removed the constructor and the app died at start
-keep class androidx.work.impl.WorkDatabase_Impl { <init>(); }
-keep class * extends androidx.room.RoomDatabase { <init>(); }

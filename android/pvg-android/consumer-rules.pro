# Proguard rules for consumers of pvg-android
-keep class com.pvg.android.** { *; }
-keepclasseswithmembernames class * {
    native <methods>;
}
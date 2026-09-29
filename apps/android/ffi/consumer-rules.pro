# JNA and the UniFFI bindings call into native code by reflection.
-dontwarn java.awt.*
-keep class com.sun.jna.* { *; }
-keepclassmembers class * extends com.sun.jna.* { public *; }
-keep class io.github.ashxtrem.hashlark.ffi.** { *; }
-keep class uniffi.** { *; }

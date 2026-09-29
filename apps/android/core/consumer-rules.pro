-keepattributes *Annotation*, InnerClasses
-dontnote kotlinx.serialization.AnnotationsKt
-keep,includedescriptorclasses class io.github.ashxtrem.hashlark.core.**$$serializer { *; }
-keepclassmembers class io.github.ashxtrem.hashlark.core.** {
    *** Companion;
}
-keepclasseswithmembers class io.github.ashxtrem.hashlark.core.** {
    kotlinx.serialization.KSerializer serializer(...);
}

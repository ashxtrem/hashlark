# R8 rules for the release build.
#
# The native library is reached through JNA and the UniFFI bindings, both by
# reflection and by name: their rules ship with the :ffi module
# (ffi/consumer-rules.pro). kotlinx.serialization keeps its serializers through
# the rules in :core (core/consumer-rules.pro).

-keepattributes *Annotation*, InnerClasses, Signature, SourceFile, LineNumberTable
-dontnote kotlinx.serialization.AnnotationsKt

# The app's own @Serializable classes (none yet; the models live in :core).
-keep,includedescriptorclasses class io.github.ashxtrem.hashlark.**$$serializer { *; }
-keepclassmembers class io.github.ashxtrem.hashlark.** {
    *** Companion;
}
-keepclasseswithmembers class io.github.ashxtrem.hashlark.** {
    kotlinx.serialization.KSerializer serializer(...);
}

# Readable stack traces in bug reports.
-renamesourcefileattribute SourceFile

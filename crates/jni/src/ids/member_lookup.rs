use crate::{
    errors::{Error, Result},
    signature::{FieldSignature, MethodSignature, RuntimeFieldSignature, RuntimeMethodSignature},
    strings::JNIStr,
};

#[cfg(doc)]
use crate::{bind_java_type, refs::Reference};

/// The kind of class member that a [`MemberLookup`] refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MemberKind {
    /// A constructor, looked up with `GetMethodID` and the name `<init>`
    Constructor,
    /// An instance method, looked up with `GetMethodID`
    Method,
    /// A static method, looked up with `GetStaticMethodID`
    StaticMethod,
    /// An instance field, looked up with `GetFieldID`
    Field,
    /// A static field, looked up with `GetStaticFieldID`
    StaticField,
}

/// Describes a method, field or constructor ID lookup that a [`bind_java_type!`]
/// binding is about to make, with the name and signature written in the
/// binding.
///
/// This is passed to a binding's `map_member` hook, which returns the name and
/// signature to look up at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MemberLookup<'a> {
    /// The kind of member being looked up
    pub kind: MemberKind,

    /// The binding's Java class name, in the same dot-separated format as
    /// [`Reference::class_name()`], such as `"com.example.MyClass"`
    pub class_name: &'a JNIStr,

    /// The member's Java name (`"<init>"` for constructors)
    pub name: &'a JNIStr,

    /// The member's JNI signature, such as `"(Ljava/lang/String;)I"` or `"I"`
    pub sig: &'a JNIStr,
}

impl MemberLookup<'_> {
    /// Parses a mapped method signature, checking that it has the same argument
    /// and return types as `expected` and that constructors aren't renamed.
    ///
    /// This is an implementation detail of `bind_java_type!`.
    #[doc(hidden)]
    pub fn __map_method_signature(
        &self,
        expected: &MethodSignature,
        name: &JNIStr,
        sig: &JNIStr,
    ) -> Result<RuntimeMethodSignature> {
        if self.kind == MemberKind::Constructor && name != self.name {
            return Err(Error::InvalidMemberMapping(format!(
                "constructor {}{} of {} can't be renamed to {name}",
                self.name, self.sig, self.class_name
            )));
        }
        let mapped = RuntimeMethodSignature::from_str(sig.to_str())?;
        let mapped_sig = mapped.method_signature();
        if mapped_sig.args() != expected.args() || mapped_sig.ret() != expected.ret() {
            return Err(Error::InvalidMemberMapping(format!(
                "{}{} of {} was mapped to {name}{sig}, which has different types",
                self.name, self.sig, self.class_name
            )));
        }
        Ok(mapped)
    }

    /// Parses a mapped field signature, checking that it has the same type as
    /// `expected`.
    ///
    /// This is an implementation detail of `bind_java_type!`.
    #[doc(hidden)]
    pub fn __map_field_signature(
        &self,
        expected: &FieldSignature,
        name: &JNIStr,
        sig: &JNIStr,
    ) -> Result<RuntimeFieldSignature> {
        let mapped = RuntimeFieldSignature::from_str(sig.to_str())?;
        if mapped.field_signature().ty() != expected.ty() {
            return Err(Error::InvalidMemberMapping(format!(
                "{} {} of {} was mapped to {name} {sig}, which has a different type",
                self.name, self.sig, self.class_name
            )));
        }
        Ok(mapped)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{jni_sig, jni_str};

    fn lookup(
        kind: MemberKind,
        name: &'static JNIStr,
        sig: &'static JNIStr,
    ) -> MemberLookup<'static> {
        let class_name = jni_str!("com.example.Readable");
        MemberLookup {
            kind,
            class_name,
            name,
            sig,
        }
    }

    #[test]
    fn test_map_method_signature() {
        let sig = jni_str!("(Lcom/example/Readable;I)[I");
        let member = lookup(MemberKind::Method, jni_str!("get"), sig);
        let expected = jni_sig!("(Lcom/example/Readable;I)[I");
        let mapped = member
            .__map_method_signature(&expected, jni_str!("a"), jni_str!("(La;I)[I"))
            .unwrap();
        assert_eq!(mapped.method_signature().sig(), jni_str!("(La;I)[I"));

        // Only class names may change
        for sig in [
            jni_str!("(La;J)[I"),
            jni_str!("(La;)[I"),
            jni_str!("(La;I)La;"),
            jni_str!("(La;I"),
        ] {
            let result = member.__map_method_signature(&expected, jni_str!("a"), sig);
            assert!(result.is_err(), "{sig} should be rejected");
        }

        // Constructors can't be renamed
        let member = lookup(MemberKind::Constructor, jni_str!("<init>"), jni_str!("()V"));
        let expected = jni_sig!("()V");
        let init = jni_str!("<init>");
        assert!(
            member
                .__map_method_signature(&expected, init, jni_str!("()V"))
                .is_ok()
        );
        assert!(
            member
                .__map_method_signature(&expected, jni_str!("a"), jni_str!("()V"))
                .is_err()
        );
    }

    #[test]
    fn test_map_field_signature() {
        let member = lookup(
            MemberKind::Field,
            jni_str!("next"),
            jni_str!("Lcom/example/Readable;"),
        );
        let expected = jni_sig!("Lcom/example/Readable;");
        let mapped = member
            .__map_field_signature(&expected, jni_str!("b"), jni_str!("La;"))
            .unwrap();
        assert_eq!(mapped.field_signature().sig(), jni_str!("La;"));

        for sig in [jni_str!("I"), jni_str!("[La;"), jni_str!("La;I")] {
            let result = member.__map_field_signature(&expected, jni_str!("b"), sig);
            assert!(result.is_err(), "{sig} should be rejected");
        }
    }
}

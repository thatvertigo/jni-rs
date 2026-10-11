#![cfg(feature = "invocation")]
//! Tests for the `bind_java_type!` `map_member` hook.
//!
//! The bindings are written against a readable `com.example.Readable` class
//! while the class that's loaded at runtime (`com.example.TestMappedMembers`)
//! has short, obfuscated member names.
mod util;

use assert_matches::assert_matches;
use jni::errors::{Error, Result};
use jni::ids::{MemberKind, MemberLookup};
use jni::objects::JClass;
use jni::refs::LoaderContext;
use jni::strings::JNIString;
use jni::{Env, bind_java_type, jni_str};
use rusty_fork::rusty_fork_test;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Maps the readable member names to the names in `TestMappedMembers.java`
fn readable_to_runtime(member: MemberLookup) -> Result<(JNIString, JNIString)> {
    let name = match (member.kind, &*member.name.to_str()) {
        (MemberKind::Constructor, "<init>") => "<init>",
        (MemberKind::StaticMethod, "add") => "a",
        (MemberKind::Method, "getValue" | "combine") => "a",
        (MemberKind::StaticField, "staticValue") => "a",
        (MemberKind::Field, "value") => "b",
        (MemberKind::Field, "next") => "c",
        (kind, name) => panic!("unexpected member: {kind:?} {name}"),
    };
    let sig = member
        .sig
        .to_str()
        .replace("com/example/Readable", "com/example/TestMappedMembers");
    Ok((name.into(), sig.into()))
}

fn load_runtime_class<'local>(
    env: &mut Env<'local>,
    load_context: &LoaderContext,
    initialize: bool,
) -> Result<JClass<'local>> {
    load_context.load_class(env, jni_str!("com.example.TestMappedMembers"), initialize)
}

static SEEN_MEMBERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

bind_java_type! {
    rust_type = Readable,
    java_type = com.example.Readable,
    constructors {
        fn new(value: jint),
    },
    methods {
        static fn add(a: jint, b: jint) -> jint,
        fn get_value() -> jint,
        fn combine(other: Readable) -> Readable,
    },
    fields {
        static static_value: jint,
        value: jint,
        next: Readable,
    },
    hooks = {
        load_class = |env, load_context, initialize| {
            load_runtime_class(env, load_context, initialize)
        },
        map_member = |_env, _load_context, _class, member| {
            SEEN_MEMBERS.lock().unwrap().push(format!(
                "{:?} {} {} {}",
                member.kind, member.class_name, member.name, member.sig
            ));
            readable_to_runtime(member)
        },
    },
}

rusty_fork_test! {
#[test]
fn test_map_member() {
    let out_dir = compile_test_class("bind_map_member");

    util::attach_current_thread(|env| {
        define_test_class(env, &out_dir);

        let a = Readable::new(env, 2)?;
        let b = Readable::new(env, 3)?;
        assert_eq!(Readable::add(env, 20, 22)?, 42);
        let c = a.combine(env, &b)?;
        assert_eq!(c.get_value(env)?, 5);

        assert_eq!(Readable::static_value(env)?, 42);
        c.set_value(env, 9)?;
        assert_eq!(c.value(env)?, 9);
        c.set_next(env, &a)?;
        let next = c.next(env)?;
        assert!(env.is_same_object(&next, &a)?);

        Ok(())
    })
    .expect("map_member test failed");

    // The hook is given the names and signatures written in the binding
    let mut seen = SEEN_MEMBERS.lock().unwrap().clone();
    seen.sort();
    assert_eq!(
        seen,
        [
            "Constructor com.example.Readable <init> (I)V",
            "Field com.example.Readable next Lcom/example/Readable;",
            "Field com.example.Readable value I",
            "Method com.example.Readable combine (Lcom/example/Readable;)Lcom/example/Readable;",
            "Method com.example.Readable getValue ()I",
            "StaticField com.example.Readable staticValue I",
            "StaticMethod com.example.Readable add (II)I",
        ]
    );
}
}

static RETRY_HOOK_CALLS: AtomicUsize = AtomicUsize::new(0);

// Fails the first time the hook is called
bind_java_type! {
    rust_type = RetryReadable,
    java_type = com.example.Readable,
    methods {
        fn get_value() -> jint,
    },
    hooks = {
        load_class = |env, load_context, initialize| {
            load_runtime_class(env, load_context, initialize)
        },
        map_member = |_env, _load_context, _class, member| {
            if RETRY_HOOK_CALLS.fetch_add(1, Ordering::SeqCst) == 0 {
                return Err(Error::ParseFailed("mappings not available".to_string()));
            }
            readable_to_runtime(member)
        },
    },
}

// Maps a method to a signature with a different return type
bind_java_type! {
    rust_type = IncompatibleReadable,
    java_type = com.example.Readable,
    methods {
        fn get_value() -> jint,
    },
    hooks = {
        load_class = |env, load_context, initialize| {
            load_runtime_class(env, load_context, initialize)
        },
        map_member = |_env, _load_context, _class, _member| {
            Ok((jni_str!("a"), jni_str!("()J")))
        },
    },
}

rusty_fork_test! {
#[test]
fn test_map_member_errors() {
    let out_dir = compile_test_class("bind_map_member_errors");

    util::attach_current_thread(|env| {
        define_test_class(env, &out_dir);
        let loader = LoaderContext::None;

        assert_matches!(
            IncompatibleReadableAPI::get(env, &loader).map(|_| ()),
            Err(Error::InvalidMemberMapping(_))
        );

        // Errors from the hook are returned, and aren't cached
        assert_matches!(
            RetryReadableAPI::get(env, &loader).map(|_| ()),
            Err(Error::ParseFailed(msg)) if msg == "mappings not available"
        );
        RetryReadableAPI::get(env, &loader)?;
        assert_eq!(RETRY_HOOK_CALLS.load(Ordering::SeqCst), 2);

        Ok(())
    })
    .expect("map_member errors test failed");
}
}

fn compile_test_class(test_name: &str) -> PathBuf {
    let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("jni_macros_tests")
        .join(test_name);
    let _ = fs::remove_dir_all(&out_dir);
    fs::create_dir_all(&out_dir).expect("Failed to create test output directory");

    javac::Build::new()
        .file("tests/java/com/example/TestMappedMembers.java")
        .output_dir(&out_dir)
        .compile();

    out_dir
}

fn define_test_class(env: &mut Env, out_dir: &std::path::Path) {
    let class_bytes = fs::read(out_dir.join("com/example/TestMappedMembers.class"))
        .expect("Failed to read TestMappedMembers.class");
    let class_loader = jni::objects::JClassLoader::get_system_class_loader(env)
        .expect("Failed to get system class loader");
    env.define_class(
        Some(jni_str!("com/example/TestMappedMembers")),
        &class_loader,
        &class_bytes,
    )
    .expect("Failed to define TestMappedMembers class");
}

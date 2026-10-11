package com.example;

/**
 * Test class for the bind_java_type `map_member` hook, with short member names
 * like an obfuscated build. The bindings use readable names (and the class name
 * `com.example.Readable`), which the hook maps to these names.
 */
public class TestMappedMembers {
    // static field `staticValue`
    public static int a = 42;

    // field `value`
    public int b;

    // field `next`
    public TestMappedMembers c;

    public TestMappedMembers(int value) {
        this.b = value;
    }

    // static method `add`
    public static int a(int x, int y) {
        return x + y;
    }

    // method `getValue`
    public int a() {
        return b;
    }

    // method `combine`
    public TestMappedMembers a(TestMappedMembers other) {
        return new TestMappedMembers(b + other.b);
    }
}

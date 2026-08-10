// ActrSharp managed-layer tests.
//
// Positive path (real native load + InitRuntime + ValidateNative + EchoWorkload
// registration + init/shutdown) is covered by the out-of-repo consumer in
// ci-csharp.yml. These tests cover the managed-only invariants that do not need
// a loaded actr_cabi.dll: exception context formatting, ActrNativeInfo shape,
// and the negative "native library missing" path (which must surface an
// ActrCabiException with the DllNotFound code + process RID rather than an
// opaque DllNotFoundException).

using ActrSharp;
using Xunit;

namespace ActrSharp.Tests;

public class ActrCabiExceptionTests
{
    [Fact]
    public void Message_includes_context_fields_when_set()
    {
        var ex = new ActrCabiException(ActrCabiException.CodeAbiMismatch, "ABI version mismatch")
        {
            Expected = "abi=1",
            Actual = "abi=2",
            Rid = "win-x64",
            Bundle = "com.test",
        };
        var msg = ex.Message;
        Assert.Contains("ABI version mismatch", msg);
        Assert.Contains("expected=abi=1", msg);
        Assert.Contains("actual=abi=2", msg);
        Assert.Contains("rid=win-x64", msg);
        Assert.Contains("bundle=com.test", msg);
    }

    [Fact]
    public void Message_omits_context_block_when_no_fields_set()
    {
        var ex = new ActrCabiException(-20, "workload not found");
        Assert.Equal("workload not found", ex.Message);
    }

    [Fact]
    public void Message_includes_partial_context()
    {
        var ex = new ActrCabiException(-31, "version mismatch")
        {
            Expected = "actr=0.7.0",
            Actual = "actr=0.6.0",
        };
        var msg = ex.Message;
        Assert.Contains("expected=actr=0.7.0", msg);
        Assert.Contains("actual=actr=0.6.0", msg);
        Assert.DoesNotContain("rid=", msg);
        Assert.DoesNotContain("bundle=", msg);
    }

    [Fact]
    public void Code_and_inner_exception_are_preserved()
    {
        var inner = new InvalidOperationException("boom");
        var ex = new ActrCabiException(ActrCabiException.CodeArchMismatch, "arch mismatch", inner);
        Assert.Equal(ActrCabiException.CodeArchMismatch, ex.Code);
        Assert.Same(inner, ex.InnerException);
    }
}

public class ActrNativeInfoTests
{
    [Fact]
    public void ToString_includes_all_fields_when_bundle_present()
    {
        var info = new ActrNativeInfo(1, "0.7.0", "com.test");
        Assert.Equal("abi=1 actr=0.7.0 bundle=com.test", info.ToString());
    }

    [Fact]
    public void ToString_shows_none_when_bundle_absent()
    {
        var info = new ActrNativeInfo(1, "0.7.0", null);
        Assert.Equal("abi=1 actr=0.7.0 bundle=<none>", info.ToString());
    }

    [Fact]
    public void Record_equality_compares_all_fields()
    {
        var a = new ActrNativeInfo(1, "0.7.0", "com.test");
        var b = new ActrNativeInfo(1, "0.7.0", "com.test");
        Assert.Equal(a, b);
    }
}

public class NativeValidationNegativeTests
{
    // This test project does not ship actr_cabi.dll, so the loader must fail.
    // SafeNative converts the loader failure to an ActrCabiException with the
    // DllNotFound code and the process RID — never an opaque DllNotFoundException.
    [Fact]
    public void InitRuntime_without_native_dll_throws_actr_exception_with_rid()
    {
        var ex = Assert.Throws<ActrCabiException>(() => ActrCabiNode.InitRuntime());
        Assert.Equal(ActrCabiException.CodeDllNotFound, ex.Code);
        Assert.False(string.IsNullOrEmpty(ex.Rid));
    }

    [Fact]
    public void ValidateNative_without_native_dll_throws_actr_exception_with_rid()
    {
        var ex = Assert.Throws<ActrCabiException>(() => ActrCabiNode.ValidateNative("cabi.DoesNotExist"));
        Assert.Equal(ActrCabiException.CodeDllNotFound, ex.Code);
        Assert.False(string.IsNullOrEmpty(ex.Rid));
    }
}

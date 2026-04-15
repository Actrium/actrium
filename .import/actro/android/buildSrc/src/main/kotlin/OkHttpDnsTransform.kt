import com.android.build.api.instrumentation.AsmClassVisitorFactory
import com.android.build.api.instrumentation.ClassContext
import com.android.build.api.instrumentation.ClassData
import com.android.build.api.instrumentation.InstrumentationParameters
import org.gradle.api.provider.Property
import org.gradle.api.tasks.Input
import org.objectweb.asm.ClassVisitor
import org.objectweb.asm.MethodVisitor
import org.objectweb.asm.Opcodes
import org.objectweb.asm.commons.AdviceAdapter

/**
 * 使用 Android Gradle Plugin 的 Transform API 在 OkHttpClient.Builder 返回前插入 .dns(CellularDns())
 */
abstract class OkHttpDnsTransform : AsmClassVisitorFactory<OkHttpDnsTransform.Parameters> {

    interface Parameters : InstrumentationParameters {
        @get:Input
        val enabled: Property<Boolean>
    }

    override fun createClassVisitor(
        classContext: ClassContext,
        nextClassVisitor: ClassVisitor
    ): ClassVisitor {
        return OkHttpDnsClassVisitor(nextClassVisitor)
    }

    override fun isInstrumentable(classData: ClassData): Boolean {
        // 只处理 React Native 的 OkHttpClientProvider
        return classData.className == "com.facebook.react.modules.network.OkHttpClientProvider"
    }

    private class OkHttpDnsClassVisitor(cv: ClassVisitor) : ClassVisitor(Opcodes.ASM9, cv) {
        override fun visitMethod(
            access: Int,
            name: String?,
            descriptor: String?,
            signature: String?,
            exceptions: Array<out String>?
        ): MethodVisitor {
            val mv = super.visitMethod(access, name, descriptor, signature, exceptions)
            // 只在 createClientBuilder() 方法插桩（无参数的静态方法）
            if (name == "createClientBuilder"
                && descriptor == "()Lokhttp3/OkHttpClient\$Builder;") {
                return OkHttpDnsMethodVisitor(mv, access, name, descriptor)
            }
            return mv
        }
    }

    private class OkHttpDnsMethodVisitor(
        mv: MethodVisitor,
        access: Int,
        name: String?,
        descriptor: String?
    ) : AdviceAdapter(Opcodes.ASM9, mv, access, name, descriptor) {

        override fun onMethodExit(opcode: Int) {
            if (opcode == ARETURN) {
                // 此时栈顶是 OkHttpClient.Builder（即将 return）
                // 我们需要在这个 builder 上调用 .dns(new CellularDns())

                // 1. NEW CellularDns
                mv.visitTypeInsn(NEW, "chat/askaway/android/core/CellularDns")
                // 栈: [builder, cellularDns]

                // 2. DUP - 复制 CellularDns 引用用于构造函数调用
                mv.visitInsn(DUP)
                // 栈: [builder, cellularDns, cellularDns]

                // 3. INVOKESPECIAL - 调用 CellularDns 无参构造函数
                mv.visitMethodInsn(
                    INVOKESPECIAL,
                    "chat/askaway/android/core/CellularDns",
                    "<init>",
                    "()V",
                    false
                )
                // 栈: [builder, cellularDns]

                // 4. INVOKEVIRTUAL - 调用 builder.dns(cellularDns)
                mv.visitMethodInsn(
                    INVOKEVIRTUAL,
                    "okhttp3/OkHttpClient\$Builder",
                    "dns",
                    "(Lokhttp3/Dns;)Lokhttp3/OkHttpClient\$Builder;",
                    false
                )
                // 栈: [builder] - 返回的还是 builder，可以继续链式调用
            }
        }
    }
}

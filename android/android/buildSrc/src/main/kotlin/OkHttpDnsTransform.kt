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
 * Use Android Gradle Plugin It's... Transform API Yes. OkHttpClient.Builder Insert Before Return .dns(CellularDns())
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
        // Only handle React Native It's... OkHttpClientProvider
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
            // Only createClientBuilder() Method pin (static method without parameters)
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
                // At this time, the top of the inn is... OkHttpClient.Builder(Pending) return)
                // We need to be here. builder Call Up .dns(new CellularDns())

                // 1. NEW CellularDns
                mv.visitTypeInsn(NEW, "io/actrium/actro/core/CellularDns")
                // The inn.: [builder, cellularDns]

                // 2. DUP - Copy CellularDns Reference for construction functions
                mv.visitInsn(DUP)
                // The inn.: [builder, cellularDns, cellularDns]

                // 3. INVOKESPECIAL - Call CellularDns No Quadratic Functions
                mv.visitMethodInsn(
                    INVOKESPECIAL,
                    "io/actrium/actro/core/CellularDns",
                    "<init>",
                    "()V",
                    false
                )
                // The inn.: [builder, cellularDns]

                // 4. INVOKEVIRTUAL - Call builder.dns(cellularDns)
                mv.visitMethodInsn(
                    INVOKEVIRTUAL,
                    "okhttp3/OkHttpClient\$Builder",
                    "dns",
                    "(Lokhttp3/Dns;)Lokhttp3/OkHttpClient\$Builder;",
                    false
                )
                // The inn.: [builder] - Returning or not? builder, you can continue to call in chains
            }
        }
    }
}

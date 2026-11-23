import games.jvrcruz.siliconcraft.compiler.n2.N2Compiler;
import java.io.DataInputStream;
import java.io.IOException;
import java.io.PrintWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

public class CompileN2 {
    public static void main(String[] args) throws IOException {
        Map<String, List<String>> files = new LinkedHashMap<>();
        try (DataInputStream input = new DataInputStream(Files.newInputStream(Path.of(args[0])))) {
            for (int count = input.readInt(); count > 0; count--) {
                files.put(readUtf8(input), readUtf8(input).lines().toList());
            }
        }
        var result = N2Compiler.compileProject(files, args[1]);
        if (!result.success()) {
            result.errors().forEach(System.err::println);
            System.exit(1);
        }
        var output = new PrintWriter(System.out, false, StandardCharsets.UTF_8);
        result.assembly().forEach(output::println);
        output.flush();
    }

    private static String readUtf8(DataInputStream input) throws IOException {
        int length = input.readInt();
        byte[] data = input.readNBytes(length);
        if (data.length != length) {
            throw new IOException("Truncated compiler input");
        }
        return new String(data, StandardCharsets.UTF_8);
    }
}

import java.io.File;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Paths;

public class java {
    public static void main(String[] args) {
        try {
            byte[] content = Files.readAllBytes(Paths.get("input.txt"));
            Files.write(Paths.get("output.txt"), content);
        } catch (IOException e) {
            e.printStackTrace();
        }
    }
}

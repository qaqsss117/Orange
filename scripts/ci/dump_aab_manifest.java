import com.android.ide.common.resources.ProtoXmlPullParser;
import java.io.BufferedInputStream;
import java.io.InputStream;
import java.nio.file.Path;
import java.util.zip.ZipEntry;
import java.util.zip.ZipFile;
import org.xmlpull.v1.XmlPullParser;

final class DumpAabManifest {
    private static final String MANIFEST_ENTRY = "base/manifest/AndroidManifest.xml";
    private static final long MAX_MANIFEST_BYTES = 4L * 1024 * 1024;

    public static void main(String[] arguments) throws Exception {
        if (arguments.length != 1) {
            throw new IllegalArgumentException("expected one Android App Bundle path");
        }

        try (ZipFile bundle = new ZipFile(Path.of(arguments[0]).toFile())) {
            ZipEntry manifest = bundle.getEntry(MANIFEST_ENTRY);
            if (manifest == null || manifest.isDirectory()) {
                throw new IllegalArgumentException("base module manifest is missing");
            }
            if (manifest.getSize() <= 0 || manifest.getSize() > MAX_MANIFEST_BYTES) {
                throw new IllegalArgumentException(
                        "base module manifest size is outside the audit limit");
            }
            try (InputStream input = new BufferedInputStream(bundle.getInputStream(manifest))) {
                System.out.print(decode(input));
            }
        }
    }

    private static String decode(InputStream input) throws Exception {
        ProtoXmlPullParser parser = new ProtoXmlPullParser();
        parser.setInput(input, null);
        StringBuilder output =
                new StringBuilder("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");

        for (int event = parser.getEventType();
                event != XmlPullParser.END_DOCUMENT;
                event = parser.nextToken()) {
            if (event == XmlPullParser.START_TAG) {
                output.append('<').append(qualifiedName(parser.getPrefix(), parser.getName()));
                int previousNamespaces = parser.getDepth() > 1
                        ? parser.getNamespaceCount(parser.getDepth() - 1)
                        : 0;
                int namespaceCount = parser.getNamespaceCount(parser.getDepth());
                for (int index = previousNamespaces; index < namespaceCount; index++) {
                    String prefix = parser.getNamespacePrefix(index);
                    output.append(" xmlns");
                    if (prefix != null && !prefix.isEmpty()) {
                        output.append(':').append(prefix);
                    }
                    output.append("=\"")
                            .append(escape(parser.getNamespaceUri(index), true))
                            .append('"');
                }
                for (int index = 0; index < parser.getAttributeCount(); index++) {
                    output.append(' ')
                            .append(qualifiedName(
                                    parser.getAttributePrefix(index),
                                    parser.getAttributeName(index)))
                            .append("=\"")
                            .append(escape(parser.getAttributeValue(index), true))
                            .append('"');
                }
                output.append('>');
            } else if (event == XmlPullParser.END_TAG) {
                output.append("</")
                        .append(qualifiedName(parser.getPrefix(), parser.getName()))
                        .append('>');
            } else if (event == XmlPullParser.TEXT && parser.getText() != null) {
                output.append(escape(parser.getText(), false));
            }
        }
        output.append('\n');
        return output.toString();
    }

    private static String qualifiedName(String prefix, String name) {
        return prefix == null || prefix.isEmpty() ? name : prefix + ":" + name;
    }

    private static String escape(String value, boolean attribute) {
        String escaped = value
                .replace("&", "&amp;")
                .replace("<", "&lt;")
                .replace(">", "&gt;");
        return attribute ? escaped.replace("\"", "&quot;") : escaped;
    }
}

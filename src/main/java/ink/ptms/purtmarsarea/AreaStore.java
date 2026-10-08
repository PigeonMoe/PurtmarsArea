package ink.ptms.purtmarsarea;

import org.bukkit.configuration.ConfigurationSection;
import org.bukkit.configuration.file.YamlConfiguration;
import java.io.*;
import java.nio.channels.FileChannel;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;

/** A malformed record fails the entire load; never silently drops protected land. */
public final class AreaStore {
    private final Path file;
    public AreaStore(Path file) { this.file = file; }
    public AreaIndex load() throws Exception {
        AreaIndex index = new AreaIndex();
        if (!Files.exists(file)) return index;
        YamlConfiguration yaml = new YamlConfiguration(); yaml.load(file.toFile());
        if (yaml.getInt("schema") != 2) throw new IOException("不支持的领地数据版本");
        ConfigurationSection root = section(yaml, "areas");
        for (String key : root.getKeys(false)) {
            ConfigurationSection s = section(root, key);
            List<?> raw = s.getList("volumes");
            if (raw == null || raw.isEmpty() || raw.size() > 257) throw new IOException("缺失或过多领地范围");
            List<Volume> volumes = new ArrayList<>();
            for (Object entry : raw) {
                if (!(entry instanceof Map<?, ?> m)) throw new IOException("无效领地范围");
                volumes.add(new Volume(number(m, "x"), number(m, "y"), number(m, "z"),
                        number(m, "rx"), number(m, "ry"), number(m, "rz"), Objects.toString(m.get("core"), "0")));
            }
            String ownerId = s.getString("owner-uuid");
            Area area = new Area(UUID.fromString(key), required(s, "world"),
                    ownerId == null ? null : UUID.fromString(ownerId), required(s, "owner-name"), volumes.getFirst());
            area.volumes.clear(); area.volumes.addAll(volumes);
            area.name = s.getString("name", area.name); area.welcome = s.getString("welcome", area.welcome);
            area.farewell = s.getString("farewell", area.farewell);
            readFlags(s.getConfigurationSection("flags"), area.flags);
            ConfigurationSection members = s.getConfigurationSection("members");
            if (members != null) for (String member : members.getKeys(false)) {
                ConfigurationSection ms = section(members, member);
                Area.Member m = new Area.Member(required(ms, "name")); readFlags(ms.getConfigurationSection("flags"), m.flags);
                area.members.put(member, m);
            }
            index.add(area);
        }
        return index;
    }
    public void save(AreaIndex index) throws IOException {
        YamlConfiguration yaml = new YamlConfiguration(); yaml.set("schema", 2); yaml.createSection("areas");
        for (Area area : index.all()) {
            ConfigurationSection s = yaml.createSection("areas." + area.id);
            s.set("world", area.world); s.set("owner-uuid", area.owner == null ? null : area.owner.toString());
            s.set("owner-name", area.ownerName); s.set("name", area.name); s.set("welcome", area.welcome); s.set("farewell", area.farewell);
            List<Map<String, Object>> volumes = new ArrayList<>();
            for (Volume v : area.volumes) {
                Map<String, Object> m = new LinkedHashMap<>();
                m.put("x", v.x()); m.put("y", v.y()); m.put("z", v.z()); m.put("rx", v.rx()); m.put("ry", v.ry()); m.put("rz", v.rz()); m.put("core", v.core()); volumes.add(m);
            }
            s.set("volumes", volumes); writeFlags(s, "flags", area.flags);
            for (Map.Entry<String, Area.Member> entry : area.members.entrySet()) {
                ConfigurationSection ms = s.createSection("members." + entry.getKey());
                ms.set("name", entry.getValue().name); writeFlags(ms, "flags", entry.getValue().flags);
            }
        }
        Files.createDirectories(file.toAbsolutePath().getParent());
        Path temp = Files.createTempFile(file.toAbsolutePath().getParent(), "areas-", ".tmp");
        try {
            Files.writeString(temp, yaml.saveToString(), StandardCharsets.UTF_8);
            try (FileChannel channel = FileChannel.open(temp, StandardOpenOption.WRITE)) { channel.force(true); }
            if (Files.exists(file)) Files.copy(file, file.resolveSibling(file.getFileName() + ".bak"), StandardCopyOption.REPLACE_EXISTING);
            try { Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE); }
            catch (AtomicMoveNotSupportedException ex) { Files.move(temp, file, StandardCopyOption.REPLACE_EXISTING); }
        } finally { Files.deleteIfExists(temp); }
    }
    /** Explicit import from 1.2 save/<world>.yml. The original files stay untouched. */
    public List<Area> readLegacy(Path folder) throws Exception {
        List<Area> result = new ArrayList<>();
        if (!Files.isDirectory(folder)) throw new IOException("请把旧 save 目录复制到插件目录下的 legacy-save");
        try (var files = Files.list(folder)) {
            for (Path path : files.filter(p -> p.getFileName().toString().endsWith(".yml")).sorted().toList()) {
                String world = path.getFileName().toString().replaceFirst("\\.yml$", "");
                YamlConfiguration yaml = new YamlConfiguration(); yaml.load(path.toFile());
                for (String location : yaml.getKeys(false)) {
                    String[] xyz = location.split(","); if (xyz.length != 3) throw new IOException("无效旧坐标：" + location);
                    ConfigurationSection s = section(yaml, location);
                    Area a = new Area(UUID.nameUUIDFromBytes((world + ":" + location).getBytes(StandardCharsets.UTF_8)), world, null,
                            required(s, "owner"), new Volume(Integer.parseInt(xyz[0]), Integer.parseInt(xyz[1]), Integer.parseInt(xyz[2]),
                            s.getInt("size.x"), s.getInt("size.y"), s.getInt("size.z"), String.valueOf(s.getInt("index"))));
                    readFlags(s.getConfigurationSection("flags"), a.flags);
                    ConfigurationSection members = s.getConfigurationSection("members");
                    if (members != null) for (String name : members.getKeys(false)) {
                        Area.Member m = new Area.Member(name); readFlags(section(members, name), m.flags);
                        a.members.put(name.toLowerCase(Locale.ROOT), m);
                    }
                    result.add(a);
                }
            }
        }
        return result;
    }
    private static int number(Map<?, ?> map, String key) throws IOException {
        Object value = map.get(key);
        if (!(value instanceof Number n) || n.doubleValue() != n.intValue()) throw new IOException("无效整数：" + key);
        return n.intValue();
    }
    private static ConfigurationSection section(ConfigurationSection parent, String key) throws IOException {
        ConfigurationSection s = parent.getConfigurationSection(key); if (s == null) throw new IOException("缺少节点：" + key); return s;
    }
    private static String required(ConfigurationSection s, String key) throws IOException {
        String value = s.getString(key); if (value == null || value.isBlank()) throw new IOException("缺少字段：" + key); return value;
    }
    private static void readFlags(ConfigurationSection s, Map<Flag, Boolean> flags) throws IOException {
        if (s == null) return;
        for (String id : s.getKeys(false)) {
            if (!s.isBoolean(id)) throw new IOException("权限必须为布尔值：" + id);
            flags.put(Flag.parse(id), s.getBoolean(id));
        }
    }
    private static void writeFlags(ConfigurationSection s, String path, Map<Flag, Boolean> flags) {
        flags.forEach((flag, value) -> s.set(path + "." + flag.id, value));
    }
}

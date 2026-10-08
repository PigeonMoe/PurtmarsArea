package ink.ptms.purtmarsarea;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import java.nio.file.*;
import java.util.*;
import static org.junit.jupiter.api.Assertions.*;

class StoreTest {
    @TempDir Path dir;
    private Area area() { return new Area(UUID.randomUUID(),"world",UUID.randomUUID(),"owner",new Volume(-17,64,32,8,8,8,"0")); }
    @Test void restartPreservesMultipleCoresUuidFlagsAndMessages() throws Exception {
        Area a = area(); a.name = "测试领地"; a.welcome = "你好 {name}"; a.flags.put(Flag.BUILD,true);
        a.volumes.add(new Volume(-1,64,32,8,8,8,"1")); Area.Member m = new Area.Member("member"); m.flags.put(Flag.BUILD,false);
        String memberId = UUID.randomUUID().toString(); a.members.put(memberId,m); AreaIndex index = new AreaIndex(); index.add(a);
        AreaStore store = new AreaStore(dir.resolve("areas.yml")); store.save(index); AreaIndex read = store.load(); Area restored = read.get(a.id);
        assertEquals(a.owner,restored.owner); assertEquals(a.volumes,restored.volumes); assertEquals(a.name,restored.name);
        assertEquals(a.welcome,restored.welcome); assertTrue(restored.flags.get(Flag.BUILD)); assertFalse(restored.members.get(memberId).flags.get(Flag.BUILD));
        assertSame(restored,read.at("world",7,64,32));
    }
    @Test void emptySaveIsValidAndPreviousSnapshotIsKept() throws Exception {
        AreaIndex index = new AreaIndex(); index.add(area()); AreaStore store = new AreaStore(dir.resolve("areas.yml")); store.save(index);
        String previous = Files.readString(dir.resolve("areas.yml")); store.save(new AreaIndex());
        assertEquals(previous,Files.readString(dir.resolve("areas.yml.bak"))); assertTrue(store.load().all().isEmpty());
    }
    @Test void corruptSchemaOrRecordFailsWholeLoadWithoutRewritingInput() throws Exception {
        Path file = dir.resolve("areas.yml"); String bad = "schema: 2\nareas:\n  not-a-uuid:\n    world: world\n"; Files.writeString(file,bad);
        assertThrows(Exception.class, () -> new AreaStore(file).load()); assertEquals(bad,Files.readString(file));
        Files.writeString(file,"schema: 99\nareas: {}\n"); assertThrows(Exception.class, () -> new AreaStore(file).load());
    }
    @Test void overlappingPersistedAreasFailClosed() throws Exception {
        Area a = area(); Area b = area(); AreaIndex index = new AreaIndex(); index.add(a); AreaStore store = new AreaStore(dir.resolve("areas.yml")); store.save(index);
        String yaml = Files.readString(dir.resolve("areas.yml")); String areaPart = yaml.substring(yaml.indexOf("  " + a.id));
        Files.writeString(dir.resolve("areas.yml"),yaml + areaPart.replace(a.id.toString(),b.id.toString()));
        assertThrows(Exception.class,store::load);
    }
    @Test void legacyImportKeepsOriginalAndDeterministicIdEvenForEmptyMemberFlags() throws Exception {
        Path legacy = dir.resolve("legacy-save"); Files.createDirectories(legacy);
        String input = "'-17,64,32':\n  owner: Original\n  index: 0\n  size: {x: 8, y: 8, z: 8}\n  flags: {build: false}\n  members:\n    Member: {}\n";
        Files.writeString(legacy.resolve("world.yml"),input); AreaStore store = new AreaStore(dir.resolve("areas.yml"));
        List<Area> imported = store.readLegacy(legacy); assertEquals(1,imported.size()); Area a = imported.getFirst();
        assertNull(a.owner); assertEquals("Original",a.ownerName); assertTrue(a.members.containsKey("member"));
        assertEquals(a.id,store.readLegacy(legacy).getFirst().id); assertEquals(input,Files.readString(legacy.resolve("world.yml")));
    }
    @Test void badLegacyInputDoesNotCreateOutput() throws Exception {
        Path legacy = dir.resolve("legacy-save"); Files.createDirectories(legacy); Files.writeString(legacy.resolve("world.yml"),"bad: {owner: x}\n");
        assertThrows(Exception.class, () -> new AreaStore(dir.resolve("areas.yml")).readLegacy(legacy)); assertFalse(Files.exists(dir.resolve("areas.yml")));
    }
}

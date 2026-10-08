package ink.ptms.purtmarsarea;

import java.util.*;

/** Index every covered chunk, including unloaded chunks. Never loads a Bukkit world. */
public final class AreaIndex {
    private record Chunk(String world, int x, int z) {}
    private final Map<Chunk, Set<Area>> chunks = new HashMap<>();
    private final Map<UUID, Area> areas = new LinkedHashMap<>();
    public Collection<Area> all() { return Collections.unmodifiableCollection(areas.values()); }
    public Area get(UUID id) { return areas.get(id); }
    public Area at(String world, int x, int y, int z) {
        return chunks.getOrDefault(new Chunk(world, x >> 4, z >> 4), Set.of()).stream()
                .filter(a -> a.contains(x, y, z)).findFirst().orElse(null);
    }
    public Set<Area> overlaps(String world, Volume v) {
        Set<Area> result = new LinkedHashSet<>();
        for (int x = v.minX() >> 4; x <= v.maxX() >> 4; x++)
            for (int z = v.minZ() >> 4; z <= v.maxZ() >> 4; z++)
                for (Area a : chunks.getOrDefault(new Chunk(world, x, z), Set.of()))
                    if (a.volumes.stream().anyMatch(v::intersects)) result.add(a);
        return result;
    }
    public void add(Area area) {
        if (areas.containsKey(area.id)) throw new IllegalArgumentException("重复领地 ID");
        for (Volume v : area.volumes) if (!overlaps(area.world, v).isEmpty())
            throw new IllegalArgumentException("领地与已有领地相交：" + area.id);
        areas.put(area.id, area); index(area);
    }
    public void reindex(Area area) { remove(area); areas.put(area.id, area); index(area); }
    private void index(Area a) {
        for (Volume v : a.volumes)
            for (int x = v.minX() >> 4; x <= v.maxX() >> 4; x++)
                for (int z = v.minZ() >> 4; z <= v.maxZ() >> 4; z++)
                    chunks.computeIfAbsent(new Chunk(a.world, x, z), k -> new LinkedHashSet<>()).add(a);
    }
    public void remove(Area area) {
        areas.remove(area.id); chunks.values().forEach(set -> set.remove(area));
        chunks.values().removeIf(Set::isEmpty);
    }
}

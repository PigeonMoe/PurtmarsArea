package ink.ptms.purtmarsarea;

import java.util.*;

public final class Area {
    public final UUID id;
    public final String world;
    public UUID owner;
    public String ownerName, name, welcome = "欢迎来到 {name}", farewell = "离开 {name}";
    public final List<Volume> volumes = new ArrayList<>();
    public final EnumMap<Flag, Boolean> flags = new EnumMap<>(Flag.class);
    // UUID for modern players; legacy import keeps names until that player logs in.
    public final Map<String, Member> members = new LinkedHashMap<>();
    public Area(UUID id, String world, UUID owner, String ownerName, Volume primary) {
        this.id = id; this.world = world; this.owner = owner; this.ownerName = ownerName;
        this.name = ownerName + "的领地"; volumes.add(primary);
    }
    public boolean isOwner(UUID id, String name) {
        return owner != null ? owner.equals(id) : ownerName.equalsIgnoreCase(name);
    }
    public Member member(UUID id, String name) {
        Member m = members.get(id.toString());
        return m != null ? m : members.get(name.toLowerCase(Locale.ROOT));
    }
    public boolean allows(Flag flag, UUID id, String playerName) {
        return allows(flag, id, playerName, flag.defaultValue);
    }
    public boolean allows(Flag flag, UUID id, String playerName, boolean configuredDefault) {
        if (isOwner(id, playerName)) return true;
        Member member = member(id, playerName);
        // Preserve original PurtmarsArea semantics: a member has independent defaults.
        if (member != null && flag.personal) return member.flags.getOrDefault(flag, configuredDefault);
        return flags.getOrDefault(flag, configuredDefault);
    }
    public boolean enabled(Flag flag) { return flags.getOrDefault(flag, flag.defaultValue); }
    public boolean contains(int x, int y, int z) { return volumes.stream().anyMatch(v -> v.contains(x, y, z)); }
    public static final class Member {
        public final String name;
        public final EnumMap<Flag, Boolean> flags = new EnumMap<>(Flag.class);
        public Member(String name) { this.name = name; }
    }
}

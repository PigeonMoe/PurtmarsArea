package ink.ptms.purtmarsarea;

import java.util.Arrays;

public enum Flag {
    PARTICLE("particle", "边界粒子", true, false),
    BUILD("build", "建筑与水桶", false, true),
    MOVE("move", "进入与移动", true, true),
    TELEPORT("teleport", "传送", true, true),
    CONTAINER("container", "容器", false, true),
    ENCHANT("enchant-table", "附魔台", true, true),
    ANVIL("anvil", "铁砧", true, true),
    DOOR("door", "门与活板门", false, true),
    CAKE("cake", "蛋糕", false, true),
    DROP("item-drop", "丢弃物品", true, true),
    PICK("item-pick", "拾取物品与箭", true, true),
    DAMAGE_ANIMALS("damage-animals", "伤害被动生物", false, true),
    DAMAGE_MONSTER("damage-monster", "伤害敌对生物", false, true),
    SPAWN_ANIMALS("spawn-animals", "被动生物生成", false, false),
    SPAWN_MONSTER("spawn-monster", "敌对生物生成", false, false),
    REDSTONE("redstone", "红石触发", false, true),
    REDSTONE_PERFORM("redstone-perform", "红石运作", true, false),
    LAVA("flow-lava", "岩浆流动", true, false),
    WATER("flow-water", "水流动", true, false),
    TRADE("villager-trade", "村民交易", true, true),
    SHEAR("shear", "修剪生物", false, true),
    LEASH("leash", "牵引生物", false, true),
    FALLING("falling", "外来方块坠落", false, false),
    EXPLOSION("explosion", "爆炸破坏", false, false),
    FIRE("fire", "火焰蔓延与燃烧", false, false),
    PVP("pvp", "玩家战斗", true, false);

    public final String id, title;
    public final boolean defaultValue, personal;
    Flag(String id, String title, boolean defaultValue, boolean personal) {
        this.id = id; this.title = title; this.defaultValue = defaultValue; this.personal = personal;
    }
    public static Flag parse(String id) {
        return Arrays.stream(values()).filter(f -> f.id.equalsIgnoreCase(id)).findFirst()
                .orElseThrow(() -> new IllegalArgumentException("未知权限：" + id));
    }
}

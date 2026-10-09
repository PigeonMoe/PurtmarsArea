package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.block.*;
import org.bukkit.block.data.*;
import org.bukkit.entity.*;
import org.bukkit.event.*;
import org.bukkit.event.block.*;
import org.bukkit.event.entity.*;
import org.bukkit.event.hanging.*;
import org.bukkit.event.inventory.*;
import org.bukkit.event.player.*;
import org.bukkit.event.vehicle.*;
import org.bukkit.inventory.*;
import org.bukkit.projectiles.ProjectileSource;
import java.util.*;

public final class ProtectionListener implements Listener {
    private final AreaService s;
    private final Map<BlockPlaceEvent, Volume> creations = new IdentityHashMap<>();
    public ProtectionListener(AreaService service) { s = service; }
    private boolean allowed(Player p, Location l, Flag f) { return s.allowed(p, l, f); }
    private boolean core(Block b) { return s.core(b.getLocation()) != null; }
    private boolean natural(Block b, Flag f) { return s.natural(b.getLocation(), f); }
    private boolean crossing(Location a, Location b) { return s.at(a) != s.at(b); }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void place(BlockPlaceEvent e) {
        if (s.cores.isBroken(e.getItemInHand())) { e.setCancelled(true); AreaService.message(e.getPlayer(),"破碎的领地核心需要修补后才能使用"); return; }
        String id = s.cores.id(e.getItemInHand());
        if (id != null && e instanceof BlockMultiPlaceEvent) { e.setCancelled(true); return; }
        if (id != null) {
            try {
                s.create(e.getPlayer(), e.getBlock().getLocation(), id);
                creations.put(e, s.cores.volume(id, e.getBlock().getLocation()));
            } catch (IllegalArgumentException ex) { e.setCancelled(true); AreaService.message(e.getPlayer(), ex.getMessage()); }
        } else if (!allowed(e.getPlayer(), e.getBlock().getLocation(), Flag.BUILD)) e.setCancelled(true);
    }
    // If a later protection plugin rejects placement, undo the provisional claim as well.
    @EventHandler(priority = EventPriority.MONITOR)
    public void placed(BlockPlaceEvent e) {
        Volume v = creations.remove(e);
        if (v != null && e.isCancelled()) {
            Area area = s.core(e.getBlock().getLocation());
            if (area != null) s.change(() -> {
                if (area.volumes.size() == 1) s.index.remove(area);
                else { area.volumes.remove(v); s.index.reindex(area); }
            });
        } else if (v != null) { Area a = s.core(e.getBlock().getLocation()); if (a != null) s.showOccupation(e.getPlayer(),a); }
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void multiPlace(BlockMultiPlaceEvent e) {
        if (s.cores.id(e.getItemInHand()) != null) { e.setCancelled(true); return; }
        for (BlockState b : e.getReplacedBlockStates())
            if (!allowed(e.getPlayer(), b.getLocation(), Flag.BUILD)) { e.setCancelled(true); return; }
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void breakBlock(BlockBreakEvent e) {
        if (core(e.getBlock())) {
            e.setCancelled(true);
            try { s.breakCore(e.getPlayer(), e.getBlock().getLocation()); }
            catch (IllegalArgumentException ex) { AreaService.message(e.getPlayer(), ex.getMessage()); }
        } else if (!allowed(e.getPlayer(), e.getBlock().getLocation(), Flag.BUILD)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void bucketFill(PlayerBucketFillEvent e) {
        if (!allowed(e.getPlayer(), e.getBlock().getLocation(), Flag.BUILD) || core(e.getBlock())) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void bucketEmpty(PlayerBucketEmptyEvent e) {
        if (!allowed(e.getPlayer(), e.getBlock().getLocation(), Flag.BUILD) || core(e.getBlock())) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST)
    public void interact(PlayerInteractEvent e) {
        if (!s.healthy) { e.setCancelled(true); s.deny(e.getPlayer(), Flag.BUILD); return; }
        Block b = e.getClickedBlock(); if (b == null) return;
        // Bukkit pre-cancels some air/physical interactions; do not overwrite DENY from another plugin.
        if (e.useInteractedBlock() == Event.Result.DENY) return;
        if (e.getAction() == Action.RIGHT_CLICK_BLOCK && core(b)) {
            e.setUseInteractedBlock(Event.Result.DENY); e.setUseItemInHand(Event.Result.DENY);
            if (e.getHand() != EquipmentSlot.HAND) return;
            try { Area a = s.core(b.getLocation()); s.requireManager(e.getPlayer(), a); s.menus.home(e.getPlayer(), a); }
            catch (IllegalArgumentException ex) { AreaService.message(e.getPlayer(), ex.getMessage()); }
            return;
        }
        Flag f = interaction(b);
        if (e.getAction() == Action.LEFT_CLICK_BLOCK && b.getType() == Material.DRAGON_EGG) f = Flag.BUILD;
        if (e.getAction() == Action.PHYSICAL && b.getType() == Material.FARMLAND) f = Flag.BUILD;
        if (f != null && !allowed(e.getPlayer(), b.getLocation(), f)) {
            e.setUseInteractedBlock(Event.Result.DENY); e.setUseItemInHand(Event.Result.DENY); return;
        }
        if (e.getAction() == Action.RIGHT_CLICK_BLOCK && e.getItem() != null && modifies(e.getItem().getType())) {
            Block adjacent = b.getRelative(e.getBlockFace());
            if (!allowed(e.getPlayer(), b.getLocation(), Flag.BUILD) || !allowed(e.getPlayer(), adjacent.getLocation(), Flag.BUILD)
                    || core(b) || core(adjacent)) { e.setUseInteractedBlock(Event.Result.DENY); e.setUseItemInHand(Event.Result.DENY); }
        }
    }
    private Flag interaction(Block b) {
        Material m = b.getType(); String name = m.name();
        if (b.getState() instanceof InventoryHolder || m == Material.JUKEBOX || m == Material.COMPOSTER || m == Material.FLOWER_POT
                || name.startsWith("POTTED_") || m == Material.CHISELED_BOOKSHELF) return Flag.CONTAINER;
        if (m == Material.ENCHANTING_TABLE) return Flag.ENCHANT;
        if (Tag.ANVIL.isTagged(m)) return Flag.ANVIL;
        if (Tag.DOORS.isTagged(m) || Tag.TRAPDOORS.isTagged(m) || Tag.FENCE_GATES.isTagged(m)) return Flag.DOOR;
        if (m == Material.CAKE || name.endsWith("_CANDLE_CAKE")) return Flag.CAKE;
        if (Tag.BUTTONS.isTagged(m) || Tag.PRESSURE_PLATES.isTagged(m) || m == Material.LEVER
                || m == Material.REPEATER || m == Material.COMPARATOR || m == Material.DAYLIGHT_DETECTOR || m == Material.TRIPWIRE) return Flag.REDSTONE;
        if (m == Material.RESPAWN_ANCHOR || m == Material.TNT || m == Material.DRAGON_EGG || Tag.BEDS.isTagged(m)
                || name.endsWith("_SIGN") || name.endsWith("_HANGING_SIGN") || m == Material.SWEET_BERRY_BUSH || m == Material.CAVE_VINES
                || m == Material.CAVE_VINES_PLANT || m == Material.BELL || m == Material.DECORATED_POT) return Flag.BUILD;
        return null;
    }
    private boolean modifies(Material m) {
        String n = m.name();
        return m.isBlock() || n.endsWith("_BUCKET") || n.endsWith("_AXE") || n.endsWith("_HOE") || n.endsWith("_SHOVEL")
                || n.endsWith("_DYE") || n.endsWith("_SPAWN_EGG") || n.endsWith("_BOAT") || n.endsWith("_RAFT") || n.endsWith("MINECART")
                || m == Material.FLINT_AND_STEEL || m == Material.FIRE_CHARGE || m == Material.BONE_MEAL || m == Material.ARMOR_STAND
                || m == Material.END_CRYSTAL || m == Material.ITEM_FRAME || m == Material.GLOW_ITEM_FRAME || m == Material.PAINTING
                || m == Material.HONEYCOMB || m == Material.GLASS_BOTTLE || m == Material.SHEARS;
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void entityInteract(PlayerInteractEntityEvent e) {
        Entity target = e.getRightClicked(); Flag f;
        if (target instanceof AbstractVillager) f = Flag.TRADE;
        else if (target instanceof InventoryHolder || target instanceof ItemFrame || target instanceof ArmorStand || target instanceof Vehicle) f = Flag.CONTAINER;
        else if (e.getPlayer().getInventory().getItem(e.getHand()).getType() == Material.LEAD) f = Flag.LEASH;
        else f = Flag.BUILD;
        if (!allowed(e.getPlayer(), target.getLocation(), f)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void entityInteractAt(PlayerInteractAtEntityEvent e) { entityInteract(e); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void armorStand(PlayerArmorStandManipulateEvent e) { if (!allowed(e.getPlayer(), e.getRightClicked().getLocation(), Flag.CONTAINER)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void hang(HangingPlaceEvent e) { if (e.getPlayer() != null && !allowed(e.getPlayer(), e.getEntity().getLocation(), Flag.BUILD)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void hangingBreak(HangingBreakEvent e) {
        Player p = e instanceof HangingBreakByEntityEvent by ? responsible(by.getRemover()) : null;
        if (p != null ? !allowed(p, e.getEntity().getLocation(), Flag.BUILD) : !s.natural(e.getEntity().getLocation(), Flag.BUILD)) e.setCancelled(true);
    }
    private Player responsible(Entity e) {
        if (e instanceof Player p) return p;
        if (e instanceof Projectile projectile && projectile.getShooter() instanceof Player p) return p;
        if (e instanceof TNTPrimed tnt && tnt.getSource() instanceof Player p) return p;
        if (e instanceof Tameable tame && tame.getOwner() instanceof Player p) return p;
        if (e instanceof AreaEffectCloud cloud && cloud.getSource() instanceof Player p) return p;
        if (e instanceof EvokerFangs fangs && fangs.getOwner() instanceof Player p) return p;
        return null;
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void damage(EntityDamageByEntityEvent e) {
        Player p = responsible(e.getDamager()); Entity target = e.getEntity();
        if (target instanceof Player) {
            if (p == null) return;
            if (!s.natural(target.getLocation(), Flag.PVP) || !s.natural(p.getLocation(), Flag.PVP)) e.setCancelled(true);
        } else {
            if (p == null && target instanceof LivingEntity) return;
            Flag f = target instanceof Monster ? Flag.DAMAGE_MONSTER : target instanceof LivingEntity ? Flag.DAMAGE_ANIMALS : Flag.BUILD;
            if (p != null ? !allowed(p, target.getLocation(), f) : !s.natural(target.getLocation(), f)) e.setCancelled(true);
        }
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void vehicleDamage(VehicleDamageEvent e) {
        Player p = responsible(e.getAttacker());
        if (p != null ? !allowed(p, e.getVehicle().getLocation(), Flag.BUILD) : !s.natural(e.getVehicle().getLocation(), Flag.BUILD)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void vehicleEnter(VehicleEnterEvent e) { if (e.getEntered() instanceof Player p && !allowed(p, e.getVehicle().getLocation(), Flag.CONTAINER)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void move(PlayerMoveEvent e) {
        Location to = e.getTo(); if (to == null || e instanceof PlayerTeleportEvent) return;
        Location from = e.getFrom();
        if (from.getBlockX() == to.getBlockX() && from.getBlockY() == to.getBlockY() && from.getBlockZ() == to.getBlockZ()) return;
        if (!allowed(e.getPlayer(), to, Flag.MOVE)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void teleport(PlayerTeleportEvent e) {
        if (e.getTo() != null && (!allowed(e.getPlayer(), e.getFrom(), Flag.TELEPORT) || !allowed(e.getPlayer(), e.getTo(), Flag.TELEPORT)
                || !allowed(e.getPlayer(), e.getTo(), Flag.MOVE))) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void transition(PlayerMoveEvent e) {
        if (e.getTo() == null) return;
        Area from = s.at(e.getFrom()), to = s.at(e.getTo()); if (from == to) return;
        s.memberInput.cancel(e.getPlayer());
        if (from != null) { Bukkit.getPluginManager().callEvent(new AreaLeaveEvent(e.getPlayer(), from));  }
        if (to != null) { Bukkit.getPluginManager().callEvent(new AreaEnterEvent(e.getPlayer(), to));  }
        if (to != null || from != null) s.showOccupation(e.getPlayer(),to != null ? to : from);
    }
    @EventHandler(priority = EventPriority.MONITOR, ignoreCancelled = true)
    public void teleportTransition(PlayerTeleportEvent e) { transition(e); }
    @EventHandler public void join(PlayerJoinEvent e) { try { s.bindLegacy(e.getPlayer()); } catch (IllegalArgumentException ex) { AreaService.message(e.getPlayer(), ex.getMessage()); } }
    @EventHandler public void quit(PlayerQuitEvent e) { s.quit(e.getPlayer()); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void drop(PlayerDropItemEvent e) { if (!allowed(e.getPlayer(), e.getItemDrop().getLocation(), Flag.DROP)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void pick(EntityPickupItemEvent e) { if (e.getEntity() instanceof Player p && !allowed(p, e.getItem().getLocation(), Flag.PICK)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void arrow(PlayerPickupArrowEvent e) { if (!allowed(e.getPlayer(), e.getArrow().getLocation(), Flag.PICK)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void shear(PlayerShearEntityEvent e) { if (!allowed(e.getPlayer(), e.getEntity().getLocation(), Flag.SHEAR)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void leash(PlayerLeashEntityEvent e) { if (!allowed(e.getPlayer(), e.getEntity().getLocation(), Flag.LEASH)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void unleash(PlayerUnleashEntityEvent e) { if (!allowed(e.getPlayer(), e.getEntity().getLocation(), Flag.LEASH)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void spawn(CreatureSpawnEvent e) {
        Flag f = e.getEntity() instanceof Monster ? Flag.SPAWN_MONSTER : Flag.SPAWN_ANIMALS;
        if (!s.natural(e.getLocation(), f)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void redstone(BlockRedstoneEvent e) { if (!natural(e.getBlock(), Flag.REDSTONE_PERFORM)) e.setNewCurrent(e.getOldCurrent()); }
    private boolean piston(Block piston, List<Block> blocks, org.bukkit.block.BlockFace direction) {
        if (core(piston) || !natural(piston, Flag.REDSTONE_PERFORM)) return false;
        Block head = piston.getRelative(direction);
        if (core(head) || crossing(piston.getLocation(), head.getLocation()) || !natural(head, Flag.REDSTONE_PERFORM)) return false;
        for (Block b : blocks) {
            Block dest = b.getRelative(direction);
            if (core(b) || core(dest) || !natural(b, Flag.REDSTONE_PERFORM) || !natural(dest, Flag.REDSTONE_PERFORM)
                    || crossing(piston.getLocation(), b.getLocation()) || crossing(b.getLocation(), dest.getLocation())) return false;
        }
        return true;
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void pistonExtend(BlockPistonExtendEvent e) { if (!piston(e.getBlock(), e.getBlocks(), e.getDirection())) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void pistonRetract(BlockPistonRetractEvent e) { if (!piston(e.getBlock(), e.getBlocks(), e.getDirection().getOppositeFace())) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void dispense(BlockDispenseEvent e) {
        if (!natural(e.getBlock(), Flag.REDSTONE_PERFORM)) { e.setCancelled(true); return; }
        if (e.getBlock().getBlockData() instanceof Directional d) {
            Block target = e.getBlock().getRelative(d.getFacing());
            if (core(target) || crossing(e.getBlock().getLocation(), target.getLocation()) || !natural(target, Flag.REDSTONE_PERFORM)) e.setCancelled(true);
        }
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void hopper(InventoryMoveItemEvent e) {
        Location from = e.getSource().getLocation(), to = e.getDestination().getLocation();
        if (from == null || to == null) return;
        if (crossing(from, to) || !s.natural(from, Flag.REDSTONE_PERFORM) || !s.natural(to, Flag.REDSTONE_PERFORM)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void hopperPickup(InventoryPickupItemEvent e) {
        Location to = e.getInventory().getLocation();
        if (to != null && (crossing(to, e.getItem().getLocation()) || !s.natural(to, Flag.REDSTONE_PERFORM))) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void open(InventoryOpenEvent e) {
        if (!(e.getPlayer() instanceof Player p) || e.getInventory().getHolder() instanceof AreaMenus.Menu) return;
        Flag f = switch (e.getInventory().getType()) { case ANVIL -> Flag.ANVIL; case ENCHANTING -> Flag.ENCHANT; case MERCHANT -> Flag.TRADE; default -> Flag.CONTAINER; };
        Location loc = e.getInventory().getLocation(); if (loc != null && !allowed(p, loc, f)) e.setCancelled(true);
        if (e.getInventory().getHolder() instanceof DoubleChest chest) {
            if (chest.getLeftSide() instanceof BlockState left && !allowed(p, left.getLocation(), f)) e.setCancelled(true);
            if (chest.getRightSide() instanceof BlockState right && !allowed(p, right.getLocation(), f)) e.setCancelled(true);
        }
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void flow(BlockFromToEvent e) {
        Flag f = e.getBlock().getType() == Material.LAVA ? Flag.LAVA : Flag.WATER;
        if (core(e.getToBlock()) || !natural(e.getBlock(), f) || !natural(e.getToBlock(), f)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void explode(EntityExplodeEvent e) { e.blockList().removeIf(b -> core(b) || !natural(b, Flag.EXPLOSION)); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void explodeBlock(BlockExplodeEvent e) { e.blockList().removeIf(b -> core(b) || !natural(b, Flag.EXPLOSION)); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void burn(BlockBurnEvent e) { if (core(e.getBlock()) || !natural(e.getBlock(), Flag.FIRE)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void ignite(BlockIgniteEvent e) {
        if (core(e.getBlock()) || (e.getPlayer() != null ? !allowed(e.getPlayer(), e.getBlock().getLocation(), Flag.BUILD) : !natural(e.getBlock(), Flag.FIRE))) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void spread(BlockSpreadEvent e) {
        if (core(e.getBlock()) || !natural(e.getBlock(), e.getSource().getType() == Material.FIRE ? Flag.FIRE : Flag.BUILD)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void entityBlock(EntityChangeBlockEvent e) {
        if (core(e.getBlock())) { e.setCancelled(true); return; }
        if (e.getEntity() instanceof FallingBlock falling) {
            Location origin = falling.getOrigin();
            if (origin == null || crossing(origin, e.getBlock().getLocation())) {
                if (!natural(e.getBlock(), Flag.FALLING)) e.setCancelled(true);
            }
        } else if (e.getEntity() instanceof Player p ? !allowed(p, e.getBlock().getLocation(), Flag.BUILD) : !natural(e.getBlock(), Flag.BUILD)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void sign(SignChangeEvent e) { if (!allowed(e.getPlayer(), e.getBlock().getLocation(), Flag.BUILD)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void fertilize(BlockFertilizeEvent e) {
        e.getBlocks().removeIf(b -> core(b.getBlock()) || (e.getPlayer() == null ? !s.natural(b.getLocation(), Flag.BUILD) : !allowed(e.getPlayer(), b.getLocation(), Flag.BUILD)));
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void grow(org.bukkit.event.world.StructureGrowEvent e) { e.getBlocks().removeIf(b -> core(b.getBlock()) || (crossing(e.getLocation(), b.getLocation()) && !s.natural(b.getLocation(), Flag.BUILD))); }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void fade(BlockFadeEvent e) { if (core(e.getBlock())) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void form(BlockFormEvent e) { if (core(e.getBlock())) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void physics(BlockPhysicsEvent e) { if (core(e.getBlock())) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void absorb(SpongeAbsorbEvent e) { e.getBlocks().removeIf(b -> core(b.getBlock()) || !s.natural(b.getLocation(), Flag.WATER)); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void potion(PotionSplashEvent e) {
        Player source = responsible(e.getPotion()); if (source == null || e.getPotion().getEffects().stream().noneMatch(this::harmful)) return;
        for (LivingEntity target : e.getAffectedEntities()) {
            if (target instanceof Player) {
                if (!s.natural(source.getLocation(), Flag.PVP) || !s.natural(target.getLocation(), Flag.PVP)) e.setIntensity(target,0);
            } else if (!allowed(source,target.getLocation(),target instanceof Monster ? Flag.DAMAGE_MONSTER : Flag.DAMAGE_ANIMALS)) e.setIntensity(target,0);
        }
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void cloud(AreaEffectCloudApplyEvent e) {
        Player source = responsible(e.getEntity()); if (source == null) return;
        var effects = new ArrayList<>(e.getEntity().getCustomEffects());
        if (e.getEntity().getBasePotionType() != null) effects.addAll(e.getEntity().getBasePotionType().getPotionEffects());
        if (effects.stream().noneMatch(this::harmful)) return;
        e.getAffectedEntities().removeIf(target -> target instanceof Player
                ? !s.natural(source.getLocation(),Flag.PVP) || !s.natural(target.getLocation(),Flag.PVP)
                : !allowed(source,target.getLocation(),target instanceof Monster ? Flag.DAMAGE_MONSTER : Flag.DAMAGE_ANIMALS));
    }

    private boolean harmful(org.bukkit.potion.PotionEffect effect) {
        return effect.getType().getCategory() == org.bukkit.potion.PotionEffectTypeCategory.HARMFUL;
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void prime(TNTPrimeEvent e) { if (core(e.getBlock()) || !natural(e.getBlock(),Flag.EXPLOSION)) e.setCancelled(true); }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void entityTrample(EntityInteractEvent e) {
        if (e.getEntity() instanceof Player p ? !allowed(p,e.getBlock().getLocation(),Flag.BUILD) : !natural(e.getBlock(),Flag.BUILD)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST)
    public void vehicleMove(VehicleMoveEvent e) {
        if (!crossing(e.getFrom(),e.getTo())) return;
        for (Entity passenger : passengers(e.getVehicle())) if (passenger instanceof Player p && !allowed(p,e.getTo(),Flag.MOVE)) {
            e.getVehicle().setVelocity(new org.bukkit.util.Vector());
            if (!e.getVehicle().teleport(e.getFrom())) {
                e.getVehicle().eject(); p.teleport(e.getFrom());
            }
            return;
        }
    }
    private List<Entity> passengers(Entity entity) {
        List<Entity> result = new ArrayList<>();
        for (Entity passenger : entity.getPassengers()) { result.add(passenger); result.addAll(passengers(passenger)); }
        return result;
    }

    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void entityPlace(EntityPlaceEvent e) {
        Location target = e.getEntity().getLocation();
        if (e.getPlayer() == null ? !s.natural(target,Flag.BUILD) : !allowed(e.getPlayer(),target,Flag.BUILD)) e.setCancelled(true);
    }
    @EventHandler(priority = EventPriority.HIGHEST, ignoreCancelled = true)
    public void mountedMove(io.papermc.paper.event.entity.EntityMoveEvent e) {
        if (!crossing(e.getFrom(),e.getTo())) return;
        for (Entity passenger : passengers(e.getEntity())) if (passenger instanceof Player p && !allowed(p,e.getTo(),Flag.MOVE)) {
            e.setCancelled(true); return;
        }
    }
}

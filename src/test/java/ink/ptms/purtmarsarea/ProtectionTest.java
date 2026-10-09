package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.block.*;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.entity.*;
import org.bukkit.event.*;
import org.bukkit.event.block.*;
import org.bukkit.event.entity.*;
import org.bukkit.event.player.*;
import org.bukkit.plugin.java.JavaPlugin;
import org.junit.jupiter.api.*;
import org.junit.jupiter.api.io.TempDir;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import java.util.logging.Logger;
import static org.mockito.Mockito.*;
import static org.junit.jupiter.api.Assertions.*;

class ProtectionTest {
    @TempDir Path dir;
    AreaService s; ProtectionListener listener; World world; Player visitor, owner; Area area;
    @BeforeEach void setup() {
        JavaPlugin plugin = mock(JavaPlugin.class); YamlConfiguration config = new YamlConfiguration();
        config.set("Settings.EnableWorld",List.of("world")); config.set("Settings.IgnoreFlags",List.of("teleport"));
        when(plugin.getConfig()).thenReturn(config); when(plugin.getDataFolder()).thenReturn(dir.toFile());
        when(plugin.getName()).thenReturn("PurtmarsArea"); when(plugin.namespace()).thenReturn("purtmarsarea"); when(plugin.getLogger()).thenReturn(Logger.getLogger("PurtmarsArea-test"));
        world = mock(World.class); when(world.getName()).thenReturn("world"); when(world.getMinHeight()).thenReturn(-64); when(world.getMaxHeight()).thenReturn(320);
        visitor = player("visitor"); owner = player("owner");
        area = new Area(UUID.randomUUID(),"world",owner.getUniqueId(),"owner",new Volume(0,64,0,8,8,8,"0"));
        s = new AreaService(plugin); s.healthy = true; s.index.add(area); listener = new ProtectionListener(s);
    }
    private Player player(String name) {
        Player p = mock(Player.class); when(p.getName()).thenReturn(name); when(p.getUniqueId()).thenReturn(UUID.randomUUID()); return p;
    }
    private Block block(int x, int y, int z) {
        Block b = mock(Block.class); when(b.getLocation()).thenReturn(new Location(world,x,y,z)); when(b.getType()).thenReturn(Material.STONE); return b;
    }
    @Test void outsiderBreakDeniedButOwnerAllowed() {
        BlockBreakEvent bad = new BlockBreakEvent(block(1,64,0),visitor); listener.breakBlock(bad); assertTrue(bad.isCancelled());
        BlockBreakEvent good = new BlockBreakEvent(block(1,64,0),owner); listener.breakBlock(good); assertFalse(good.isCancelled());
    }
    @Test void bucketUsesActualTargetAndConcreteEventTypes() {
        Block target = block(8,64,0), clicked = block(9,64,0);
        PlayerBucketEmptyEvent e = new PlayerBucketEmptyEvent(visitor,target,clicked,BlockFace.WEST,Material.WATER_BUCKET,null,org.bukkit.inventory.EquipmentSlot.HAND);
        listener.bucketEmpty(e); assertTrue(e.isCancelled());
        PlayerBucketFillEvent f = new PlayerBucketFillEvent(visitor,target,clicked,BlockFace.WEST,Material.BUCKET,null,org.bukkit.inventory.EquipmentSlot.HAND);
        listener.bucketFill(f); assertTrue(f.isCancelled());
    }
    @Test void teleportChecksBothOriginAndDestination() {
        area.flags.put(Flag.TELEPORT,false);
        PlayerTeleportEvent out = new PlayerTeleportEvent(visitor,new Location(world,1,64,0),new Location(world,20,64,0)); listener.teleport(out); assertTrue(out.isCancelled());
        PlayerTeleportEvent in = new PlayerTeleportEvent(visitor,new Location(world,20,64,0),new Location(world,1,64,0)); listener.teleport(in); assertTrue(in.isCancelled());
    }
    @Test void deniedMoveIsCancelledAndWildernessExitAllowed() {
        area.flags.put(Flag.MOVE,false); PlayerMoveEvent in = new PlayerMoveEvent(visitor,new Location(world,9,64,0),new Location(world,8,64,0)); listener.move(in); assertTrue(in.isCancelled());
        PlayerMoveEvent out = new PlayerMoveEvent(visitor,new Location(world,8,64,0),new Location(world,9,64,0)); listener.move(out); assertFalse(out.isCancelled());
    }
    @Test void sourceAndDestinationLiquidFlagsAreCheckedAndCoreNeverFlooded() {
        area.flags.put(Flag.WATER,false); Block from = block(9,64,0), to = block(8,64,0); when(from.getType()).thenReturn(Material.WATER);
        BlockFromToEvent e = new BlockFromToEvent(from,to); listener.flow(e); assertTrue(e.isCancelled());
        area.flags.put(Flag.WATER,true); BlockFromToEvent core = new BlockFromToEvent(from,block(0,64,0)); listener.flow(core); assertTrue(core.isCancelled());
    }
    @Test void outsidePistonCannotPushIntoClaimEvenWhenRedstoneEnabled() {
        Block piston = block(10,64,0), moved = block(9,64,0), dest = block(8,64,0);
        when(piston.getRelative(BlockFace.WEST)).thenReturn(moved); when(moved.getRelative(BlockFace.WEST)).thenReturn(dest);
        BlockPistonExtendEvent e = new BlockPistonExtendEvent(piston,List.of(moved),BlockFace.WEST); listener.pistonExtend(e); assertTrue(e.isCancelled());
    }
    @Test void pistonCannotMoveCoreEvenForPublicBuildAndRedstone() {
        area.flags.put(Flag.BUILD,true); Block piston = block(2,64,0), moved = block(1,64,0), dest = block(0,64,0);
        when(piston.getRelative(BlockFace.WEST)).thenReturn(moved); when(moved.getRelative(BlockFace.WEST)).thenReturn(dest);
        BlockPistonExtendEvent e = new BlockPistonExtendEvent(piston,List.of(moved),BlockFace.WEST); listener.pistonExtend(e); assertTrue(e.isCancelled());
    }
    @Test void projectilesUseShooterForAnimalDamageWithoutUnsafeCasts() {
        Arrow arrow = mock(Arrow.class); when(arrow.getShooter()).thenReturn(visitor); Cow cow = mock(Cow.class); when(cow.getLocation()).thenReturn(new Location(world,1,64,0));
        EntityDamageByEntityEvent e = mock(EntityDamageByEntityEvent.class); when(e.getDamager()).thenReturn(arrow); when(e.getEntity()).thenReturn(cow);
        listener.damage(e); verify(e).setCancelled(true);
    }
    @Test void membersCannotToggleLockedFlagsOrEnvironmentalRules() {
        assertThrows(IllegalArgumentException.class, () -> s.mutableFlag(Flag.TELEPORT,false));
        assertThrows(IllegalArgumentException.class, () -> s.mutableFlag(Flag.EXPLOSION,true));
        assertDoesNotThrow(() -> s.mutableFlag(Flag.BUILD,true));
    }
    @Test void malformedReloadKeepsLastIndexAndLocksAllActions() throws Exception {
        Files.writeString(dir.resolve("areas.yml"),"schema: 99\nareas: {}\n"); s.reload(); assertFalse(s.healthy);
        assertSame(area,s.at(new Location(world,0,64,0))); assertFalse(s.allowed(owner,new Location(world,30,64,0),Flag.BUILD));
        assertThrows(IllegalArgumentException.class, () -> s.change(() -> s.index.remove(area)));
        assertTrue(Files.readString(dir.resolve("areas.yml")).contains("schema: 99"));
    }
    @Test void persistenceFailureRollsBackMemoryToLastCommittedSnapshot() throws Exception {
        s.store.save(s.index); Files.createDirectory(dir.resolve("areas.yml.bak")); Files.writeString(dir.resolve("areas.yml.bak/block"),"block");
        assertThrows(IllegalArgumentException.class, () -> s.change(() -> area.flags.put(Flag.BUILD,true)));
        assertFalse(s.index.get(area.id).enabled(Flag.BUILD)); assertTrue(s.healthy);
    }
    @Test void everyRegisteredEventHasStaticHandlerListAndNoAbstractBucket() throws Exception {
        for (Method method : ProtectionListener.class.getDeclaredMethods()) if (method.isAnnotationPresent(EventHandler.class)) {
            Class<?> type = method.getParameterTypes()[0]; Method handlers = type.getMethod("getHandlerList");
            assertTrue(Modifier.isStatic(handlers.getModifiers()),type.getName()); assertNotNull(handlers.invoke(null));
            assertNotEquals(PlayerBucketEvent.class,type);
        }
        assertNotNull(AreaEnterEvent.getHandlerList()); assertNotNull(AreaLeaveEvent.getHandlerList());
    }

    @Test void entityPlacementChecksActualSpawnLocationAcrossBoundary() {
        Boat boat = mock(Boat.class); when(boat.getLocation()).thenReturn(new Location(world,8,64,0));
        EntityPlaceEvent event = new EntityPlaceEvent(boat,visitor,block(9,64,0),BlockFace.WEST,org.bukkit.inventory.EquipmentSlot.HAND);
        listener.entityPlace(event); assertTrue(event.isCancelled());
    }
    @Test void mountedAnimalCannotCarryVisitorPastMoveRestriction() {
        area.flags.put(Flag.MOVE,false); Horse horse = mock(Horse.class); when(horse.getPassengers()).thenReturn(List.of(visitor));
        var event = new io.papermc.paper.event.entity.EntityMoveEvent(horse,new Location(world,9,64,0),new Location(world,8,64,0));
        listener.mountedMove(event); assertTrue(event.isCancelled());
    }
    @Test void transitionTitlesUseEnteredOwnerAndLeftOwnerAndIgnoreMovementInsideSameClaim() {
        try (var bukkit = mockStatic(Bukkit.class)) {
            var manager = mock(org.bukkit.plugin.PluginManager.class); bukkit.when(Bukkit::getPluginManager).thenReturn(manager);
            listener.transition(new PlayerMoveEvent(visitor,new Location(world,20,64,0),new Location(world,1,64,0)));
            verify(visitor).sendTitle("§6私人领地","§7已被 §fowner§7 占领.",10,40,10);
            listener.transition(new PlayerMoveEvent(visitor,new Location(world,1,64,0),new Location(world,2,64,0)));
            verify(visitor,times(1)).sendTitle(anyString(),anyString(),anyInt(),anyInt(),anyInt());
            s.quit(visitor); listener.transition(new PlayerMoveEvent(visitor,new Location(world,1,64,0),new Location(world,20,64,0)));
            verify(visitor,times(2)).sendTitle("§6私人领地","§7已被 §fowner§7 占领.",10,40,10);
            Area next = new Area(UUID.randomUUID(),"world",UUID.randomUUID(),"AnotherOwner",Volume.sized(40,64,0,8,8,8,"0")); s.index.add(next);
            listener.transition(new PlayerMoveEvent(visitor,new Location(world,1,64,0),new Location(world,40,64,0)));
            verify(visitor).sendTitle("§6私人领地","§7已被 §fAnotherOwner§7 占领.",10,40,10);
            verify(manager,times(2)).callEvent(any(AreaEnterEvent.class)); verify(manager,times(2)).callEvent(any(AreaLeaveEvent.class));
        }
    }
}

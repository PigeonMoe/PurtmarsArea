package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.block.Block;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.entity.Player;
import org.bukkit.event.block.BlockPlaceEvent;
import org.bukkit.inventory.ItemStack;
import org.bukkit.plugin.java.JavaPlugin;
import org.junit.jupiter.api.*;
import org.junit.jupiter.api.io.TempDir;
import java.nio.file.Path;
import java.util.*;
import java.util.logging.Logger;
import static org.mockito.Mockito.*;
import static org.junit.jupiter.api.Assertions.*;

class CoreServiceTest {
    @TempDir Path dir;
    JavaPlugin plugin; CoreItems items; AreaService s; World world; Player owner, other; YamlConfiguration config;
    @BeforeEach void setup() {
        plugin = mock(JavaPlugin.class); items = mock(CoreItems.class); config = new YamlConfiguration();
        config.set("Settings.EnableWorld",List.of("world")); config.set("Settings.AreaLimit",1); config.set("Settings.MaxExtensions",2);
        when(plugin.getConfig()).thenReturn(config); when(plugin.getDataFolder()).thenReturn(dir.toFile()); when(plugin.getLogger()).thenReturn(Logger.getLogger("test"));
        world = mock(World.class); when(world.getName()).thenReturn("world");
        owner = player("Owner"); other = player("Other");
        when(items.volume(eq("0"),any())).thenAnswer(i -> { Location l = i.getArgument(1); return new Volume(l.getBlockX(),64,l.getBlockZ(),8,8,8,"0"); });
        s = new AreaService(plugin,items); s.healthy = true;
    }
    private Player player(String name) {
        Player p = mock(Player.class); when(p.getUniqueId()).thenReturn(UUID.randomUUID()); when(p.getName()).thenReturn(name);
        when(p.hasPermission("PurtmarsArea.create")).thenReturn(true); return p;
    }
    private Location loc(int x) { return new Location(world,x,64,0); }
    @Test void newClaimPersistsAndUsesConfiguredDefaults() throws Exception {
        config.set("Settings.Flags.build",true); s.create(owner,loc(0),"0"); Area a = s.at(loc(0));
        assertTrue(a.enabled(Flag.BUILD)); assertEquals(owner.getUniqueId(),a.owner); assertEquals(1,s.store.load().all().size());
    }
    @Test void matchingOwnerOverlapExtendsOneClaimAndRetainsPermissions() {
        s.create(owner,loc(0),"0"); Area a = s.at(loc(0)); a.flags.put(Flag.CONTAINER,true);
        s.create(owner,loc(16),"0"); assertEquals(1,s.index.all().size()); assertEquals(2,a.volumes.size());
        assertSame(a,s.at(loc(24))); assertTrue(a.enabled(Flag.CONTAINER));
    }
    @Test void differentOwnerOverlapDeniedEvenWithAdminPermission() {
        s.create(owner,loc(0),"0"); when(other.hasPermission("PurtmarsArea.command.admin")).thenReturn(true);
        assertThrows(IllegalArgumentException.class, () -> s.create(other,loc(16),"0")); assertEquals(1,s.index.all().size());
    }
    @Test void claimAndExtensionLimitsEnforced() {
        s.create(owner,loc(0),"0"); assertThrows(IllegalArgumentException.class, () -> s.create(owner,loc(40),"0"));
        s.create(owner,loc(16),"0"); s.create(owner,loc(32),"0");
        assertThrows(IllegalArgumentException.class, () -> s.create(owner,loc(48),"0")); assertNull(s.at(loc(48)));
    }
    @Test void twoIndependentClaimsCannotBeSilentlyMerged() {
        config.set("Settings.AreaLimit",10); s.create(owner,loc(0),"0"); s.create(owner,loc(32),"0");
        assertThrows(IllegalArgumentException.class, () -> s.create(owner,loc(16),"0")); assertEquals(2,s.index.all().size());
    }
    @Test void disabledWorldAndCreatePermissionAreEnforced() {
        config.set("Settings.EnableWorld",List.of("other")); assertThrows(IllegalArgumentException.class, () -> s.create(owner,loc(0),"0"));
        when(owner.hasPermission("PurtmarsArea.bypass.world")).thenReturn(true); s.create(owner,loc(0),"0");
        when(other.hasPermission("PurtmarsArea.create")).thenReturn(false); assertThrows(IllegalArgumentException.class, () -> s.create(other,loc(40),"0"));
    }
    @Test void removingPrimaryRequiresRemovingExtensionsFirst() {
        s.create(owner,loc(0),"0"); s.create(owner,loc(16),"0");
        assertThrows(IllegalArgumentException.class, () -> s.breakCore(owner,loc(0))); assertEquals(2,s.at(loc(0)).volumes.size());
    }
    @Test void laterCancelledPlacementRollsBackClaimOnDiskAndInMemory() throws Exception {
        BlockPlaceEvent event = mock(BlockPlaceEvent.class); Block block = mock(Block.class); ItemStack item = mock(ItemStack.class);
        when(block.getLocation()).thenReturn(loc(0)); when(event.getBlock()).thenReturn(block); when(event.getItemInHand()).thenReturn(item);
        when(event.getPlayer()).thenReturn(owner); when(items.id(item)).thenReturn("0");
        ProtectionListener listener = new ProtectionListener(s); listener.place(event); assertNotNull(s.at(loc(0)));
        when(event.isCancelled()).thenReturn(true); listener.placed(event);
        assertNull(s.at(loc(0))); assertTrue(s.store.load().all().isEmpty());
    }
    @Test void laterCancelledExtensionRetainsPrimaryAndItsFlags() throws Exception {
        s.create(owner,loc(0),"0"); Area a = s.at(loc(0)); a.flags.put(Flag.CONTAINER,true);
        BlockPlaceEvent event = mock(BlockPlaceEvent.class); Block block = mock(Block.class); ItemStack item = mock(ItemStack.class);
        when(block.getLocation()).thenReturn(loc(16)); when(event.getBlock()).thenReturn(block); when(event.getItemInHand()).thenReturn(item);
        when(event.getPlayer()).thenReturn(owner); when(items.id(item)).thenReturn("0");
        ProtectionListener listener = new ProtectionListener(s); listener.place(event); when(event.isCancelled()).thenReturn(true); listener.placed(event);
        assertEquals(1,a.volumes.size()); assertNull(s.at(loc(24))); assertTrue(s.store.load().get(a.id).enabled(Flag.CONTAINER));
    }
    @Test void nonOwnerCannotManageAndAdministratorCan() {
        s.create(owner,loc(0),"0"); Area a = s.at(loc(0));
        assertThrows(IllegalArgumentException.class, () -> s.requireManager(other,a));
        when(other.hasPermission("PurtmarsArea.command.admin")).thenReturn(true); assertDoesNotThrow(() -> s.requireManager(other,a));
    }
    @Test void staleMenuReferenceCannotWriteAfterReload() throws Exception {
        s.create(owner,loc(0),"0"); Area a = s.at(loc(0)); s.reload(); assertTrue(s.healthy);
        assertThrows(IllegalArgumentException.class, () -> s.requireManager(owner,a));
    }
    @Test void wildernessRuleOnlyBlocksConfiguredActions() {
        config.set("Settings.ClaimWorld.world",List.of("build")); assertFalse(s.allowed(owner,loc(100),Flag.BUILD));
        assertTrue(s.allowed(owner,loc(100),Flag.MOVE)); s.create(owner,loc(100),"0"); assertTrue(s.allowed(owner,loc(100),Flag.BUILD));
    }
    @Test void invalidCommandValuesNeverChangePermissionsAndAdminCommandsRequirePermission() {
        s.create(owner,loc(0),"0"); when(owner.getLocation()).thenReturn(loc(0)); AreaCommands commands = new AreaCommands(s);
        commands.onCommand(owner,null,"pa",new String[]{"set","build","garbage"}); assertFalse(s.at(loc(0)).enabled(Flag.BUILD));
        commands.onCommand(other,null,"pa",new String[]{"reload"}); verify(plugin,never()).reloadConfig();
        assertFalse(commands.onTabComplete(other,null,"pa",new String[]{""}).contains("item"));
    }

    @Test void removingOnlyCorePersistsDeletionAndDropsConfiguredItem() throws Exception {
        s.create(owner,loc(0),"0"); Block block = mock(Block.class); when(world.getBlockAt(any(Location.class))).thenReturn(block);
        ItemStack drop = mock(ItemStack.class); when(items.drop("0")).thenReturn(drop);
        s.breakCore(owner,loc(0)); assertNull(s.at(loc(0))); assertTrue(s.store.load().all().isEmpty());
        verify(block).setType(Material.AIR); verify(world).dropItemNaturally(any(Location.class),same(drop));
    }
    @Test void removingExtensionRetainsPrimaryProtection() throws Exception {
        s.create(owner,loc(0),"0"); s.create(owner,loc(16),"0"); Block block = mock(Block.class); when(world.getBlockAt(any(Location.class))).thenReturn(block);
        when(items.drop("0")).thenReturn(mock(ItemStack.class)); s.breakCore(owner,loc(16));
        assertNotNull(s.at(loc(0))); assertNull(s.at(loc(24))); assertEquals(1,s.store.load().all().iterator().next().volumes.size());
    }
}

package ink.ptms.purtmarsarea;

import org.bukkit.*;
import org.bukkit.block.*;
import org.bukkit.entity.*;
import org.bukkit.configuration.file.YamlConfiguration;
import org.bukkit.event.block.BlockExplodeEvent;
import org.bukkit.event.entity.EntityExplodeEvent;
import org.bukkit.plugin.java.JavaPlugin;
import org.junit.jupiter.api.*;
import org.junit.jupiter.api.io.TempDir;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;
import java.nio.file.Path;
import java.util.*;
import java.util.logging.Logger;
import static org.junit.jupiter.api.Assertions.*;
import static org.mockito.Mockito.*;

class ExplosionTest {
    @TempDir Path dir;
    World world; AreaService s; Area a; ProtectionListener listener;
    @BeforeEach void setup() {
        JavaPlugin plugin = mock(JavaPlugin.class); when(plugin.getConfig()).thenReturn(new YamlConfiguration()); when(plugin.getDataFolder()).thenReturn(dir.toFile());
        when(plugin.getLogger()).thenReturn(Logger.getLogger("explosion-test")); s = new AreaService(plugin,mock(CoreItems.class)); s.healthy = true;
        world = mock(World.class); when(world.getName()).thenReturn("world");
        a = new Area(UUID.randomUUID(),"world",UUID.randomUUID(),"Owner",Volume.sized(-17,75,27,8,8,8,"0")); s.index.add(a);
        listener = new ProtectionListener(s);
    }
    Block block(int x, int y, int z) {
        Block b = mock(Block.class); when(b.getLocation()).thenReturn(new Location(world,x,y,z)); when(b.getType()).thenReturn(Material.STONE); return b;
    }
    EntityExplodeEvent explosion(EntityType type, Location origin, Block... affected) {
        Entity source = mock(Entity.class); when(source.getType()).thenReturn(type);
        // Deliberately report a different entity location: the event's blast location is authoritative.
        when(source.getLocation()).thenReturn(new Location(world,1000,0,1000));
        return new EntityExplodeEvent(source,origin,new ArrayList<>(List.of(affected)),1,ExplosionResult.DESTROY);
    }
    @ParameterizedTest @CsvSource({"-21,71,23", "-14,78,30", "-21,75,27", "-14,75,27", "-17,71,27", "-17,78,27", "-17,75,23", "-17,75,30"})
    void externalCreeperHittingAnyInclusiveFaceCancelsWholeTerrainOperation(int x, int y, int z) {
        Block supportOutside = block(-22,70,27), protectedBlock = block(x,y,z);
        EntityExplodeEvent e = explosion(EntityType.CREEPER,new Location(world,-22,74,27),supportOutside,protectedBlock);
        listener.explode(e); assertTrue(e.isCancelled()); assertTrue(e.blockList().isEmpty());
        String report = s.explosionReports().getFirst(); assertTrue(report.contains("CREEPER | world -22,74,27"));
        assertTrue(report.contains("候选方块 2 | 受保护 1")); assertTrue(report.contains("首个命中 world "+x+","+y+","+z));
    }
    @Test void allUnclaimedCandidatesKeepVanillaExplosionAndYield() {
        Block outside = block(-22,75,27), below = block(-17,70,27); EntityExplodeEvent e = explosion(EntityType.CREEPER,new Location(world,-22,75,27),outside,below);
        listener.explode(e); assertFalse(e.isCancelled()); assertEquals(List.of(outside,below),e.blockList()); assertEquals(1,e.getYield());
        assertTrue(s.explosionReports().getFirst().contains("未命中保护范围"));
    }
    @Test void explicitAllowRetainsOrdinaryBlocksButCoreAlwaysCancelsTerrainDamage() {
        a.flags.put(Flag.EXPLOSION,true); Block inside = block(-16,75,27), outside = block(-22,75,27);
        EntityExplodeEvent allowed = explosion(EntityType.CREEPER,new Location(world,-17,75,27),inside,outside);
        listener.explode(allowed); assertFalse(allowed.isCancelled()); assertEquals(2,allowed.blockList().size());
        EntityExplodeEvent core = explosion(EntityType.CREEPER,new Location(world,-17,75,27),inside,block(-17,75,27));
        listener.explode(core); assertTrue(core.isCancelled()); assertTrue(core.blockList().isEmpty());
    }
    @Test void blastStartingInAllowedAreaCannotDamageAdjacentProtectedArea() {
        a.flags.put(Flag.EXPLOSION,true); Area protectedArea = new Area(UUID.randomUUID(),"world",UUID.randomUUID(),"Other",Volume.sized(-9,75,27,8,8,8,"1")); s.index.add(protectedArea);
        EntityExplodeEvent e = explosion(EntityType.TNT,new Location(world,-14,75,27),block(-14,75,27),block(-13,75,27));
        listener.explode(e); assertTrue(e.isCancelled()); assertTrue(e.blockList().isEmpty());
    }
    @Test void internalTntAndVerticalBoundaryBlastAreProtected() {
        EntityExplodeEvent inside = explosion(EntityType.TNT,new Location(world,-17,75,27),block(-16,75,27)); listener.explode(inside); assertTrue(inside.isCancelled());
        EntityExplodeEvent below = explosion(EntityType.CREEPER,new Location(world,-17,70,27),block(-17,70,27),block(-17,71,27));
        listener.explode(below); assertTrue(below.isCancelled()); assertTrue(below.blockList().isEmpty());
    }
    @Test void blockExplosionUsesAffectedBlocksAcrossClaimBoundary() {
        Block origin = block(-22,75,27); BlockExplodeEvent e = new BlockExplodeEvent(origin,mock(BlockState.class),new ArrayList<>(List.of(origin,block(-21,75,27))),1,ExplosionResult.DESTROY);
        listener.explodeBlock(e); assertTrue(e.isCancelled()); assertTrue(e.blockList().isEmpty()); assertTrue(s.explosionReports().getFirst().contains("BLOCK:STONE"));
    }
    @Test void anotherWorldDoesNotMatchClaimAndEmptyBlastIsAllowed() {
        World other = mock(World.class); when(other.getName()).thenReturn("other"); Block b = mock(Block.class); when(b.getLocation()).thenReturn(new Location(other,-17,75,27));
        EntityExplodeEvent e = explosion(EntityType.CREEPER,new Location(other,-17,75,27),b); listener.explode(e); assertFalse(e.isCancelled()); assertEquals(1,e.blockList().size());
        EntityExplodeEvent empty = explosion(EntityType.CREEPER,new Location(world,-17,75,27)); listener.explode(empty); assertFalse(empty.isCancelled());
    }
    @Test void legacyRadiusAndMissingExplosionFlagUseProtectedDefaultAfterReload() throws Exception {
        s.index.remove(a); Area old = new Area(a.id,"world",a.owner,"Owner",new Volume(-33,75,27,8,8,8,"0")); s.index.add(old); s.save(); s.reload(); assertTrue(s.healthy);
        EntityExplodeEvent e = explosion(EntityType.CREEPER,new Location(world,-42,75,27),block(-42,75,27),block(-41,67,19));
        listener.explode(e); assertTrue(e.isCancelled()); assertTrue(e.blockList().isEmpty());
    }
    @Test void lockedStateCancelsExplosionAndReportsStayBoundedReadOnlyAndAdminOnly() {
        s.healthy = false; EntityExplodeEvent e = explosion(EntityType.CREEPER,new Location(world,100,75,27),block(100,75,27)); listener.explode(e); assertTrue(e.isCancelled());
        for (int i=0;i<12;i++) s.recordExplosion("CREEPER",new Location(world,i,75,27),1,0,null);
        assertEquals(10,s.explosionReports().size()); assertTrue(s.explosionReports().getFirst().contains("world 11,75,27"));
        assertThrows(UnsupportedOperationException.class,() -> s.explosionReports().clear());
        Player player = mock(Player.class); new AreaCommands(s).onCommand(player,null,"pa",new String[]{"explosions"});
        verify(player).sendMessage("§e[PurtmarsArea] §f需要管理员权限");
        assertFalse(new AreaCommands(s).onTabComplete(player,null,"pa",new String[]{""}).contains("explosions"));
    }
}

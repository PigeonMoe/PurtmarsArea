package ink.ptms.purtmarsarea;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.params.ParameterizedTest;
import org.junit.jupiter.params.provider.CsvSource;
import java.util.UUID;
import static org.junit.jupiter.api.Assertions.*;

class GeometryTest {
    private Area area(String world, Volume v) { return new Area(UUID.randomUUID(), world, UUID.randomUUID(), "owner", v); }
    @ParameterizedTest @CsvSource({"-8,-8,-8,true", "8,8,8,true", "9,0,0,false", "0,-9,0,false", "0,0,9,false", "0,0,0,true"})
    void inclusiveBlockBounds(int x, int y, int z, boolean expected) { assertEquals(expected, new Volume(0,0,0,8,8,8,"0").contains(x,y,z)); }
    @Test void crossingSlabsOverlapWithoutAnyContainedCorner() {
        Volume a = new Volume(0,0,0,20,1,1,"0"), b = new Volume(0,0,0,1,20,1,"0");
        assertTrue(a.intersects(b)); assertTrue(b.intersects(a));
    }
    @Test void touchingBlocksConflictButAdjacentBlocksDoNot() {
        Volume a = new Volume(0,0,0,8,8,8,"0");
        assertTrue(a.intersects(new Volume(16,0,0,8,8,8,"0")));
        assertFalse(a.intersects(new Volume(17,0,0,8,8,8,"0")));
    }
    @ParameterizedTest @CsvSource({"-128", "-17", "-16", "-1", "0", "15", "16", "128"})
    void indexCoversNegativeAndPositiveChunkBoundaries(int x) {
        AreaIndex index = new AreaIndex(); Area a = area("world", new Volume(x,64,x,20,8,20,"0")); index.add(a);
        assertSame(a, index.at("world", x-20,56,x+20)); assertSame(a, index.at("world",x+20,72,x-20));
        assertNull(index.at("other",x,64,x)); assertNull(index.at("world",x,73,x));
    }
    @Test void extensionReindexAndRemovalDoNotLeaveGhostProtection() {
        AreaIndex index = new AreaIndex(); Area a = area("world", new Volume(0,64,0,8,8,8,"0")); index.add(a);
        Volume extension = new Volume(16,64,0,8,8,8,"0"); a.volumes.add(extension); index.reindex(a);
        assertSame(a,index.at("world",24,64,0)); a.volumes.remove(extension); index.reindex(a); assertNull(index.at("world",24,64,0));
        index.remove(a); assertTrue(index.all().isEmpty()); assertNull(index.at("world",0,64,0));
    }
    @Test void overlapsAreRejectedAcrossAnUnloadedChunkAndInAnotherWorldAreAllowed() {
        AreaIndex index = new AreaIndex(); index.add(area("world",new Volume(0,64,0,32,8,32,"0")));
        assertThrows(IllegalArgumentException.class, () -> index.add(area("world",new Volume(40,64,0,16,8,16,"0"))));
        assertDoesNotThrow(() -> index.add(area("other",new Volume(0,64,0,32,8,32,"0"))));
    }
    @Test void radiiAndCoordinatesAreBounded() {
        assertThrows(IllegalArgumentException.class, () -> new Volume(0,0,0,0,8,8,"0"));
        assertThrows(IllegalArgumentException.class, () -> new Volume(0,0,0,129,8,8,"0"));
        assertThrows(IllegalArgumentException.class, () -> new Volume(Integer.MAX_VALUE,0,0,8,8,8,"0"));
    }
}

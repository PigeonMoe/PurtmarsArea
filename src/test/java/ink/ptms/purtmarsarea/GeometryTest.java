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
    @ParameterizedTest @CsvSource({"8,-4,3", "16,-8,7", "32,-16,15", "64,-32,31"})
    void exactSidesHaveExplicitEvenAnchors(int n, int low, int high) {
        Volume v = Volume.sized(-17,64,-16,n,n,n,"0");
        assertEquals(n,v.sizeX()); assertEquals(n,v.sizeY()); assertEquals(n,v.sizeZ());
        assertEquals(-17+low,v.minX()); assertEquals(-17+high,v.maxX());
        assertTrue(v.contains(v.minX(),v.minY(),v.minZ())); assertTrue(v.contains(v.maxX(),v.maxY(),v.maxZ()));
        assertFalse(v.contains(v.minX()-1,64,-16)); assertFalse(v.contains(-17,v.maxY()+1,-16));
        assertFalse(v.intersects(Volume.sized(-17+n,64,-16,n,n,n,"0")));
        assertTrue(v.intersects(Volume.sized(-17+n-1,64,-16,n,n,n,"0")));
        AreaIndex index = new AreaIndex(); Area a = area("world",v); index.add(a);
        assertSame(a,index.at("world",v.minX(),v.minY(),v.minZ())); assertSame(a,index.at("world",v.maxX(),v.maxY(),v.maxZ()));
    }
    @Test void containmentAndVerticalIntersectionConflict() {
        Volume large = Volume.sized(0,64,0,64,64,64,"3"), small = Volume.sized(0,64,0,8,8,8,"0");
        assertTrue(large.intersects(small)); assertTrue(small.intersects(large));
        assertFalse(small.intersects(Volume.sized(0,72,0,8,8,8,"0")));
        assertTrue(small.intersects(Volume.sized(0,71,0,8,8,8,"0")));
    }
}

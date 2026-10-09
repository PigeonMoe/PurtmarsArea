package ink.ptms.purtmarsarea;

/** Absolute inclusive block bounds. Legacy radii are converted once, never reinterpreted. */
public record Volume(int x, int y, int z, int minX, int minY, int minZ,
                     int maxX, int maxY, int maxZ, String core) {
    public Volume {
        if (Math.abs((long)x) > 30_000_000 || Math.abs((long)z) > 30_000_000 || Math.abs((long)y) > 2048)
            throw new IllegalArgumentException("坐标超出范围");
        if (minX > x || maxX < x || minY > y || maxY < y || minZ > z || maxZ < z
                || (long)maxX-minX+1 > 257 || (long)maxY-minY+1 > 257 || (long)maxZ-minZ+1 > 257)
            throw new IllegalArgumentException("无效的领地边界");
    }
    /** Compatibility constructor for schema 2 and the original radius-based size configuration. */
    public Volume(int x, int y, int z, int rx, int ry, int rz, String core) {
        this(x,y,z,x-radius(rx),y-radius(ry),z-radius(rz),x+rx,y+ry,z+rz,core);
    }
    private static int radius(int r) {
        if (r < 1 || r > 128) throw new IllegalArgumentException("旧格式领地半径必须为 1..128"); return r;
    }
    public static Volume sized(int x, int y, int z, int sx, int sy, int sz, String core) {
        length(sx); length(sy); length(sz);
        return new Volume(x,y,z,x-sx/2,y-sy/2,z-sz/2,x+(sx-1)/2,y+(sy-1)/2,z+(sz-1)/2,core);
    }
    private static void length(int n) { if (n < 1 || n > 256) throw new IllegalArgumentException("领地边长必须为 1..256 个方块"); }
    public int sizeX() { return maxX-minX+1; }
    public int sizeY() { return maxY-minY+1; }
    public int sizeZ() { return maxZ-minZ+1; }
    public int rx() { return Math.max(x-minX,maxX-x); }
    public int ry() { return Math.max(y-minY,maxY-y); }
    public int rz() { return Math.max(z-minZ,maxZ-z); }
    public boolean contains(int bx, int by, int bz) {
        return bx >= minX && bx <= maxX && by >= minY && by <= maxY && bz >= minZ && bz <= maxZ;
    }
    public boolean intersects(Volume b) {
        return minX <= b.maxX && maxX >= b.minX && minY <= b.maxY && maxY >= b.minY
                && minZ <= b.maxZ && maxZ >= b.minZ;
    }
    public boolean isCore(int bx, int by, int bz) { return x == bx && y == by && z == bz; }
}

package ink.ptms.purtmarsarea;

/** Inclusive block coordinates; the whole final block belongs to the claim. */
public record Volume(int x, int y, int z, int rx, int ry, int rz, String core) {
    public Volume {
        if (rx < 1 || ry < 1 || rz < 1 || rx > 128 || ry > 128 || rz > 128)
            throw new IllegalArgumentException("领地半径必须为 1..128");
        if (Math.abs((long)x) > 30_000_000 || Math.abs((long)z) > 30_000_000 || Math.abs((long)y) > 2048)
            throw new IllegalArgumentException("坐标超出范围");
    }
    public int minX() { return x - rx; }
    public int maxX() { return x + rx; }
    public int minY() { return y - ry; }
    public int maxY() { return y + ry; }
    public int minZ() { return z - rz; }
    public int maxZ() { return z + rz; }
    public boolean contains(int bx, int by, int bz) {
        return bx >= minX() && bx <= maxX() && by >= minY() && by <= maxY() && bz >= minZ() && bz <= maxZ();
    }
    public boolean intersects(Volume b) {
        return minX() <= b.maxX() && maxX() >= b.minX() && minY() <= b.maxY() && maxY() >= b.minY()
                && minZ() <= b.maxZ() && maxZ() >= b.minZ();
    }
    public boolean isCore(int bx, int by, int bz) { return x == bx && y == by && z == bz; }
}

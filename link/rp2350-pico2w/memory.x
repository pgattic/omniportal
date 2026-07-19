MEMORY {
    /*
     * RP2350 / Pico 2 W linker map.
     *
     * Pico 2 W has 4 MiB QSPI flash. Reserve the top 128 KiB for future
     * OmniPortal storage and link firmware in the lower flash region.
     */
    FLASH_BOOT : ORIGIN = 0x10000000, LENGTH = 4K
    FLASH : ORIGIN = 0x10001000, LENGTH = 3964K
    RAM : ORIGIN = 0x20000000, LENGTH = 512K
    SRAM8 : ORIGIN = 0x20080000, LENGTH = 4K
    SRAM9 : ORIGIN = 0x20081000, LENGTH = 4K
}

SECTIONS {
    .start_block : ALIGN(4)
    {
        __start_block_addr = .;
        KEEP(*(.start_block));
    } > FLASH_BOOT
} INSERT BEFORE .vector_table;

SECTIONS {
    .end_block : ALIGN(4)
    {
        __end_block_addr = .;
        KEEP(*(.end_block));
        __flash_binary_end = .;
    } > FLASH
} INSERT AFTER .uninit;

PROVIDE(start_to_end = __end_block_addr - __start_block_addr);
PROVIDE(end_to_start = __start_block_addr - __end_block_addr);

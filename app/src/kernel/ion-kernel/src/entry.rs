//! Main entry point

use alloc::string::String;
use pc_keyboard::KeyState;

#[cfg(feature = "test")]
use super::trivial_assertion;
use crate::{arch::interrupts::keyboard::{KEYBOARD, SC_QUEUE}, asyncio, disk::{DiskId, write_at}, event, io::{self, Read, concurrent::write}, log::{self, error, info}, scheduler, vfs::{self, fd::open, fs::{IonFSVersion, alloc::{alloc_data, alloc_inode_table}, parse::{alloc_inode, read_inode, write_inode}}, structures::InodeKind}};
#[allow(unused)]
use crate::{
    c_lib, hlt_loop, arch::interrupts, lib_alloc, serial_println,
    test::run_tests,
    text,
    thread::{
        self, ThreadBuilder,
        build::TPriority,
        sync::{Mutex, MutexGuard},
    },
};

#[allow(static_mut_refs)]
fn handle_kbd() -> ! {
    {
        let sc_q = unsafe { SC_QUEUE.assume_init_ref() };
        loop {
            thread::block_until_true(|| {
                !sc_q.is_empty()
            });

            let sc = if let Some(v) =  sc_q.pop() {
                v
            } else {
                thread::yield_now();
                continue;
            };

            let ev: pc_keyboard::KeyEvent = match KEYBOARD
                .lock()
                .add_byte(sc)
            {
                Ok(Some(ev)) => ev,
                Err(e) => {
                    error!("invalid keyboard input: {e:?}");
                    thread::yield_now();
                    continue;
                }
                Ok(None) => {
                    error!("invalid keyboard input: failed to decode");
                    thread::yield_now();
                    continue;
                }
            };

            event::run_kbd_hooks(ev.clone());
            
            event::run_kbd_once_hooks(ev);

            event::wake_kbd_wakers();

            unsafe { scheduler::wake_kbd() };
        }
    }
}

#[thread_local]
static __TLOCAL: u64 = 0xdeadbeef;

/// final init.
pub fn kernel_main() {
    assert_eq!(__TLOCAL, 0xdeadbeef, "tls is not setup correctly");

    #[cfg(debug_assertions)]
    event::add_key_listener(|ev| {
        if ev.state != KeyState::Up {
            serial_println!("Key Pressed: {:?}", ev)
        }
    });

    // // keyboard input
    // ThreadBuilder::<!>::new()
    //     .with_name("keyboard_handler")
    //     .with_priority(TPriority::High)
    //     .with_stack(16 * 1024)
    //     .with_entry(handle_kbd)
    //     .build();

    // the above thread causes a #GP, #DF, or even a triple fault on QEMU.
    // so for now, we don't run it.
    // obviously, its crucial for keyboard input to work, so this must be fixed later.

    let disk0 = crate::disk::get_disk(DiskId::new(0).unwrap());

    asyncio::run_future((async || {        
        let mut root = read_inode(disk0, 0).await.unwrap();
        log::debug!("Root inode: {:#?}", root);
        if root.kind() != InodeKind::Directory {
            error!("Root inode is not a directory!");
            return;
        }

        // Only initialize filesystem if root has no data allocated
        // (meaning xtask hasn't created it yet)
        if root.data_offset() == 0 {
            let data = alloc_data(disk0, 512).await.unwrap();

            root.set_total_data_size(512);
            root.set_data_offset(data);
            root.set_kind(InodeKind::Directory);

            // Foo

            let foo_idx = alloc_inode(disk0).await.unwrap();
            let mut foo_inode = read_inode(disk0, foo_idx).await.unwrap();
            let foo_data = alloc_data(disk0, 13).await.unwrap();


            foo_inode.set_data_offset(foo_data);
            foo_inode.set_kind(InodeKind::File);
            foo_inode.set_total_data_size(13);
            
            foo_inode.write_as_file(disk0, b"Hello, world!").await.unwrap();

            write_inode(disk0, foo_idx, &foo_inode).await.unwrap();

            // Bar

            let bar_idx = alloc_inode(disk0).await.unwrap();
            let mut bar_inode = read_inode(disk0, bar_idx).await.unwrap();
            let bar_data = alloc_data(disk0, 13).await.unwrap();

            bar_inode.set_data_offset(bar_data);
            bar_inode.set_kind(InodeKind::File);
            bar_inode.set_total_data_size(13);
            
            bar_inode.write_as_file(disk0, b"world, Hello?").await.unwrap();

            write_inode(disk0, bar_idx, &bar_inode).await.unwrap();

            // Foobar directory

            let foobar_idx = alloc_inode(disk0).await.unwrap();
            let mut foobar_inode = read_inode(disk0, foobar_idx).await.unwrap();
            let foobar_data = alloc_data(disk0, 512).await.unwrap();

            foobar_inode.set_data_offset(foobar_data);
            foobar_inode.set_kind(InodeKind::Directory);
            foobar_inode.set_total_data_size(512);

            foobar_inode.write_as_dir(disk0, &[
                (b"foo.txt", foo_idx),
                (b"bar.txt", bar_idx),
            ]).await.unwrap();

            write_inode(disk0, foobar_idx, &foobar_inode).await.unwrap();

            // Write root directory with foobar subdirectory

            root.write_as_dir(disk0, &[
                (b"foobar", foobar_idx),
            ]).await.unwrap();

            write_inode(disk0, 0, &root).await.unwrap();
        }

        log::debug!("Attempting to read @/main.txt");
        let mut foo = io::fs::OpenOptions::new()
            .read(true)
            .open("@/main.txt")
            .await
            .unwrap();

        let mut buf = String::new();

        use io::concurrent::read::AsyncRead;

        foo.async_read_to_string(&mut buf).await.unwrap();
        log::info!("Read from @/main.txt: \n{}", buf);
    })());


    cfg_if::cfg_if! {
        if #[cfg(feature = "test")] {
            use crate::stack;
            run_tests(&[
                // all tests go here

                // control, test for tests
                &trivial_assertion,
                // interrupts
                &interrupts::test::test_breakpoint,
                // VGA
                &text::test_println_output,
                // Alloc
                &lib_alloc::tests::test_large_alloc,
                &lib_alloc::tests::test_freed_mem_used,
                &lib_alloc::tests::test_alloc_tools,
                // C Lib / LibC
                &c_lib::libc::mem::test_malloc,
                // Stack
                &stack::tests::tracked_initial_rsp_is_top,
                &stack::tests::tracked_map_preserves_rsp,
                &stack::tests::tracked_rejects_misaligned_rsp,
                &stack::tests::tracked_rejects_out_of_bounds_rsp,
                &stack::tests::tracked_set_rsp_rejects_invalid,
                &stack::tests::tracked_unchecked_preserves_rsp,
                &stack::tests::tracked_used_remaining_correct,
            ]);
        } else {
            thread::exit_current();
        }
    }
}

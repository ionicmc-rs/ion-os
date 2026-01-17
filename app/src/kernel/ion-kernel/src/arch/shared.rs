
/// Represents the width of a pointer
/// 
/// is 16, 32 or 64
#[derive(Debug, Clone, Copy)]
pub enum PointerWidth {
    /// 16
    Sixteen,
    /// 32
    ThirtyTwo,
    /// 64
    SixtyFour
}
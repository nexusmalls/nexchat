use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use core::fmt::Debug;

/// 分页请求参数
#[derive(
    Encode,
    Decode,
    codec::DecodeWithMemTracking,
    Clone,
    Copy,
    PartialEq,
    Eq,
    TypeInfo,
    MaxEncodedLen,
    Debug,
)]
pub struct PageRequest {
    /// 起始偏移量（0-indexed）
    pub offset: u32,
    /// 每页数量（上限由各接口自行限制）
    pub limit: u32,
}

impl Default for PageRequest {
    fn default() -> Self {
        Self {
            offset: 0,
            limit: 20,
        }
    }
}

impl PageRequest {
    /// 创建分页请求
    pub fn new(offset: u32, limit: u32) -> Self {
        Self { offset, limit }
    }

    /// 限制 limit 不超过最大值
    pub fn capped(self, max_limit: u32) -> Self {
        Self {
            offset: self.offset,
            limit: self.limit.min(max_limit),
        }
    }
}

/// 分页响应
#[derive(
    Encode, Decode, codec::DecodeWithMemTracking, Clone, PartialEq, Eq, TypeInfo, Debug,
)]
pub struct PageResponse<T> {
    /// 当前页数据
    pub items: sp_std::vec::Vec<T>,
    /// 总记录数
    pub total: u32,
    /// 是否有更多数据
    pub has_more: bool,
}

impl<T> PageResponse<T> {
    /// 创建空分页响应
    pub fn empty() -> Self {
        Self {
            items: sp_std::vec::Vec::new(),
            total: 0,
            has_more: false,
        }
    }

    /// 从完整列表构建分页响应
    pub fn from_slice(all_items: sp_std::vec::Vec<T>, page: &PageRequest) -> Self {
        let total = all_items.len() as u32;
        let start = (page.offset as usize).min(all_items.len());
        let end = start
            .saturating_add(page.limit as usize)
            .min(all_items.len());
        let has_more = end < all_items.len();
        let items = all_items
            .into_iter()
            .skip(start)
            .take(end - start)
            .collect();
        Self {
            items,
            total,
            has_more,
        }
    }
}

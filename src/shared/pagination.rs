//! Pagination utilities
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Pagination parameters extracted from request
#[derive(Debug, Clone, Deserialize)]
pub struct PaginationParams {
    /// Page number (1-indexed)
    pub page: Option<u32>,

    /// Number of items per page
    pub limit: Option<u32>,

    /// Number of items to skip (overrides page calculation)
    pub offset: Option<u32>,

    /// Field to sort by
    pub sort_by: Option<String>,

    /// Sort direction (asc, desc)
    pub sort_order: Option<String>,

    /// Custom filters
    #[serde(flatten)]
    pub filters: HashMap<String, String>,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: Some(1),
            limit: Some(20),
            offset: None,
            sort_by: None,
            sort_order: Some("asc".to_string()),
            filters: HashMap::new(),
        }
    }
}

/// Pagination information for response
#[derive(Debug, Clone, Serialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub limit: u32,
    pub total: u64,
    pub total_pages: u32,
    pub has_next: bool,
    pub has_prev: bool,
    pub offset: u32,
    pub next_page: Option<u32>,
    pub prev_page: Option<u32>,
}

impl PaginationInfo {
    /// Create pagination information
    pub fn new(page: u32, limit: u32, total: u64) -> Self {
        let total_pages = if total == 0 {
            0
        } else {
            ((total as f64) / (limit as f64)).ceil() as u32
        };

        let has_next = page < total_pages;
        let has_prev = page > 1;
        let offset = (page - 1) * limit;

        let next_page = if has_next { Some(page + 1) } else { None };
        let prev_page = if has_prev { Some(page - 1) } else { None };

        Self {
            page,
            limit,
            total,
            total_pages,
            has_next,
            has_prev,
            offset,
            next_page,
            prev_page,
        }
    }

    /// Create pagination information from offset
    pub fn from_offset(offset: u32, limit: u32, total: u64) -> Self {
        let page = if limit > 0 {
            (offset / limit) + 1
        } else {
            1
        };
        Self::new(page, limit, total)
    }

    /// Get database LIMIT value
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// Get database OFFSET value
    pub fn offset(&self) -> u32 {
        (self.page - 1) * self.limit
    }
}

/// Paginated response wrapper
#[derive(Debug, Clone, Serialize)]
pub struct PaginatedResponse<T: Serialize> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

impl<T: Serialize> PaginatedResponse<T> {
    /// Create a new paginated response
    pub fn new(data: Vec<T>, pagination: PaginationInfo) -> Self {
        Self { data, pagination }
    }

    /// Create paginated response from page/limit/total
    pub fn from_page(data: Vec<T>, page: u32, limit: u32, total: u64) -> Self {
        let pagination = PaginationInfo::new(page, limit, total);
        Self::new(data, pagination)
    }

    /// Create paginated response from offset
    pub fn from_offset(data: Vec<T>, offset: u32, limit: u32, total: u64) -> Self {
        let pagination = PaginationInfo::from_offset(offset, limit, total);
        Self::new(data, pagination)
    }

    /// Get the number of items in current page
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if current page is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

/// Sort direction enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    /// Parse string to SortDirection
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "desc" | "descending" => SortDirection::Desc,
            _ => SortDirection::Asc,
        }
    }

    /// Convert to string
    pub fn as_str(&self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }

    /// Reverse the direction
    pub fn reverse(&self) -> Self {
        match self {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        }
    }
}

impl Default for SortDirection {
    fn default() -> Self {
        SortDirection::Asc
    }
}

/// Sorting information
#[derive(Debug, Clone, Serialize)]
pub struct SortInfo {
    pub field: String,
    pub direction: SortDirection,
}

impl SortInfo {
    /// Create new sort info
    pub fn new(field: String, direction: SortDirection) -> Self {
        Self { field, direction }
    }

    /// Create from field and optional direction string
    pub fn from_params(field: Option<String>, direction: Option<String>) -> Option<Self> {
        field.map(|f| {
            let dir = direction
                .map(|d| SortDirection::from_str(&d))
                .unwrap_or_default();
            Self::new(f, dir)
        })
    }

    /// Get SQL ORDER BY clause
    pub fn to_order_by(&self) -> String {
        format!("{} {}", self.field, self.direction.as_str())
    }
}

/// Query builder for pagination and sorting
#[derive(Debug, Clone)]
pub struct PaginationBuilder {
    params: PaginationParams,
}

impl PaginationBuilder {
    /// Create new pagination builder
    pub fn new(params: PaginationParams) -> Self {
        Self { params }
    }

    /// Get page number with default
    pub fn page(&self) -> u32 {
        self.params.page.unwrap_or(1).max(1)
    }

    /// Get limit with validation and default
    pub fn limit(&self) -> u32 {
        let limit = self.params.limit.unwrap_or(20);
        limit.clamp(1, 100) // Between 1 and 100
    }

    /// Get offset calculated from page
    pub fn offset(&self) -> u32 {
        self.params
            .offset
            .unwrap_or((self.page() - 1) * self.limit())
            .max(0)
    }

    /// Get sort information
    pub fn sort(&self) -> Option<SortInfo> {
        SortInfo::from_params(
            self.params.sort_by.clone(),
            self.params.sort_order.clone(),
        )
    }

    /// Get filters
    pub fn filters(&self) -> &HashMap<String, String> {
        &self.params.filters
    }

    /// Get specific filter value
    pub fn filter(&self, key: &str) -> Option<&String> {
        self.params.filters.get(key)
    }

    /// Build SQL LIMIT clause
    pub fn sql_limit(&self) -> String {
        format!("LIMIT {} OFFSET {}", self.limit(), self.offset())
    }

    /// Build SQL ORDER BY clause
    pub fn sql_order_by(&self) -> Option<String> {
        self.sort().map(|s| s.to_order_by())
    }

    /// Build complete pagination SQL
    pub fn sql_pagination(&self) -> String {
        let mut sql = String::new();

        if let Some(order_by) = self.sql_order_by() {
            sql.push_str(&format!("ORDER BY {} ", order_by));
        }

        sql.push_str(&self.sql_limit());

        sql
    }

    /// Calculate pagination info for response
    pub fn pagination_info(&self, total: u64) -> PaginationInfo {
        PaginationInfo::new(self.page(), self.limit(), total)
    }
}

/// Extension trait for extracting pagination from query parameters
pub trait QueryPaginationExt {
    /// Extract pagination parameters
    fn pagination(&self) -> PaginationParams;
}

impl QueryPaginationExt for axum::extract::Query<HashMap<String, String>> {
    fn pagination(&self) -> PaginationParams {
        let mut params = PaginationParams::default();

        for (key, value) in self.0.iter() {
            match key.as_str() {
                "page" => {
                    if let Ok(page) = value.parse::<u32>() {
                        params.page = Some(page);
                    }
                }
                "limit" => {
                    if let Ok(limit) = value.parse::<u32>() {
                        params.limit = Some(limit);
                    }
                }
                "offset" => {
                    if let Ok(offset) = value.parse::<u32>() {
                        params.offset = Some(offset);
                    }
                }
                "sort_by" => {
                    params.sort_by = Some(value.clone());
                }
                "sort_order" => {
                    params.sort_order = Some(value.clone());
                }
                _ => {
                    // Add to filters
                    params.filters.insert(key.clone(), value.clone());
                }
            }
        }

        params
    }
}

/// Convenience functions for common pagination scenarios
pub mod utils {
    use super::*;

    /// Create default pagination parameters
    pub fn default_params() -> PaginationParams {
        PaginationParams::default()
    }

    /// Create pagination parameters with custom defaults
    pub fn params_with_defaults(
        page: Option<u32>,
        limit: Option<u32>,
        sort_by: Option<String>,
    ) -> PaginationParams {
        PaginationParams {
            page: page.or(Some(1)),
            limit: limit.or(Some(20)),
            sort_by,
            sort_order: Some("asc".to_string()),
            ..Default::default()
        }
    }

    /// Create pagination info for first page
    pub fn first_page(limit: u32, total: u64) -> PaginationInfo {
        PaginationInfo::new(1, limit, total)
    }

    /// Check if page number is valid
    pub fn is_valid_page(page: u32, total_pages: u32) -> bool {
        page >= 1 && (total_pages == 0 || page <= total_pages)
    }

    /// Calculate total pages from total count and limit
    pub fn calculate_total_pages(total: u64, limit: u32) -> u32 {
        if limit == 0 {
            return 0;
        }
        ((total as f64) / (limit as f64)).ceil() as u32
    }

    /// Get page numbers for pagination UI (with ellipsis)
    pub fn get_page_numbers(
        current_page: u32,
        total_pages: u32,
        window_size: u32,
    ) -> Vec<u32> {
        if total_pages == 0 {
            return vec![];
        }

        let mut pages = Vec::new();
        let half_window = window_size / 2;

        // Always include first page
        if current_page > half_window + 1 {
            pages.push(1);
        }

        // Calculate window around current page
        let start = (current_page.saturating_sub(half_window) + 1).min(total_pages);
        let end = (current_page + half_window).min(total_pages);

        // Add ellipsis if needed before window
        if start > 2 && !pages.contains(&1) {
            pages.push(1);
            // Note: You might want to add ellipsis handling here
        }

        // Add window pages
        for page in start..=end {
            if !pages.contains(&page) {
                pages.push(page);
            }
        }

        // Always include last page
        if end < total_pages && !pages.contains(&total_pages) {
            pages.push(total_pages);
        }

        pages.sort();
        pages.dedup();
        pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_pagination_params_default() {
        let params = PaginationParams::default();
        assert_eq!(params.page, Some(1));
        assert_eq!(params.limit, Some(20));
        assert_eq!(params.offset, None);
        assert_eq!(params.sort_order, Some("asc".to_string()));
    }

    #[test]
    fn test_pagination_info() {
        let info = PaginationInfo::new(2, 20, 55);
        assert_eq!(info.page, 2);
        assert_eq!(info.limit, 20);
        assert_eq!(info.total, 55);
        assert_eq!(info.total_pages, 3);
        assert!(info.has_next);
        assert!(info.has_prev);
        assert_eq!(info.next_page, Some(3));
        assert_eq!(info.prev_page, Some(1));
        assert_eq!(info.offset, 20);
    }

    #[test]
    fn test_pagination_info_first_page() {
        let info = PaginationInfo::new(1, 20, 15);
        assert_eq!(info.page, 1);
        assert_eq!(info.total_pages, 1);
        assert!(!info.has_next);
        assert!(!info.has_prev);
        assert_eq!(info.next_page, None);
        assert_eq!(info.prev_page, None);
        assert_eq!(info.offset, 0);
    }

    #[test]
    fn test_pagination_info_empty() {
        let info = PaginationInfo::new(1, 20, 0);
        assert_eq!(info.page, 1);
        assert_eq!(info.total_pages, 0);
        assert!(!info.has_next);
        assert!(!info.has_prev);
        assert_eq!(info.offset, 0);
    }

    #[test]
    fn test_sort_direction() {
        assert_eq!(SortDirection::from_str("asc"), SortDirection::Asc);
        assert_eq!(SortDirection::from_str("ASC"), SortDirection::Asc);
        assert_eq!(SortDirection::from_str("desc"), SortDirection::Desc);
        assert_eq!(SortDirection::from_str("DESC"), SortDirection::Desc);
        assert_eq!(SortDirection::from_str("invalid"), SortDirection::Asc);
        assert_eq!(SortDirection::from_str(""), SortDirection::Asc);

        assert_eq!(SortDirection::Asc.as_str(), "ASC");
        assert_eq!(SortDirection::Desc.as_str(), "DESC");

        assert_eq!(SortDirection::Asc.reverse(), SortDirection::Desc);
        assert_eq!(SortDirection::Desc.reverse(), SortDirection::Asc);
    }

    #[test]
    fn test_sort_info() {
        let sort = SortInfo::new("name".to_string(), SortDirection::Desc);
        assert_eq!(sort.field, "name");
        assert_eq!(sort.to_order_by(), "name DESC");

        let sort = SortInfo::from_params(
            Some("email".to_string()),
            Some("asc".to_string()),
        ).unwrap();
        assert_eq!(sort.field, "email");
        assert_eq!(sort.to_order_by(), "email ASC");
    }

    #[test]
    fn test_pagination_builder() {
        let params = PaginationParams {
            page: Some(2),
            limit: Some(10),
            sort_by: Some("name".to_string()),
            sort_order: Some("desc".to_string()),
            ..Default::default()
        };

        let builder = PaginationBuilder::new(params);
        assert_eq!(builder.page(), 2);
        assert_eq!(builder.limit(), 10);
        assert_eq!(builder.offset(), 10);

        let sort = builder.sort().unwrap();
        assert_eq!(sort.field, "name");
        assert_eq!(sort.to_order_by(), "name DESC");
    }

    #[test]
    fn test_pagination_builder_limits() {
        let params = PaginationParams {
            page: Some(0),    // Should be clamped to 1
            limit: Some(150), // Should be clamped to 100
            // No negative `offset` case: the field is `Option<u32>`, so the type already makes a
            // negative offset unrepresentable — there is nothing to clamp. `offset` is derived from
            // the clamped page/limit below.
            ..Default::default()
        };

        let builder = PaginationBuilder::new(params);
        assert_eq!(builder.page(), 1);
        assert_eq!(builder.limit(), 100);
        assert_eq!(builder.offset(), 0);
    }

    #[test]
    fn test_paginated_response() {
        let data = vec!["item1", "item2", "item3"];
        let response = PaginatedResponse::from_page(data.clone(), 1, 10, 100);

        assert_eq!(response.data, data);
        assert_eq!(response.pagination.page, 1);
        assert_eq!(response.pagination.limit, 10);
        assert_eq!(response.pagination.total, 100);
        assert_eq!(response.pagination.total_pages, 10);
        assert!(response.pagination.has_next);
        assert!(!response.pagination.has_prev);
        assert_eq!(response.len(), 3);
        assert!(!response.is_empty());
    }

    #[test]
    fn test_query_pagination_ext() {
        let mut query_map = HashMap::new();
        query_map.insert("page".to_string(), "2".to_string());
        query_map.insert("limit".to_string(), "15".to_string());
        query_map.insert("sort_by".to_string(), "name".to_string());
        query_map.insert("sort_order".to_string(), "desc".to_string());
        query_map.insert("filter".to_string(), "value".to_string());

        let query = axum::extract::Query(query_map);
        let params = query.pagination();

        assert_eq!(params.page, Some(2));
        assert_eq!(params.limit, Some(15));
        assert_eq!(params.sort_by, Some("name".to_string()));
        assert_eq!(params.sort_order, Some("desc".to_string()));
        assert_eq!(params.filters.get("filter"), Some(&"value".to_string()));
    }

    #[test]
    fn test_pagination_utils() {
        assert_eq!(utils::calculate_total_pages(0, 10), 0);
        assert_eq!(utils::calculate_total_pages(5, 10), 1);
        assert_eq!(utils::calculate_total_pages(10, 10), 1);
        assert_eq!(utils::calculate_total_pages(11, 10), 2);
        assert_eq!(utils::calculate_total_pages(100, 10), 10);

        assert!(utils::is_valid_page(1, 5));
        assert!(utils::is_valid_page(5, 5));
        assert!(!utils::is_valid_page(6, 5));
        assert!(!utils::is_valid_page(0, 5));
    }
}
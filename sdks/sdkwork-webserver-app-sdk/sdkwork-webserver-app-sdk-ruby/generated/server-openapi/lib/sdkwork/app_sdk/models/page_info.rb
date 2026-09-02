module Sdkwork
  module AppSdk
    module Models
      class PageInfo
              attr_accessor :mode, :page, :page_size, :total_items, :total_pages, :next_cursor, :has_more

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @mode = attributes['mode']
                @page = attributes['page']
                @page_size = attributes['pageSize']
                @total_items = attributes['totalItems']
                @total_pages = attributes['totalPages']
                @next_cursor = attributes['nextCursor']
                @has_more = attributes['hasMore']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'mode' => @mode,
                  'page' => @page,
                  'pageSize' => @page_size,
                  'totalItems' => @total_items,
                  'totalPages' => @total_pages,
                  'nextCursor' => @next_cursor,
                  'hasMore' => @has_more,
                }
              end
            end
    end
  end
end

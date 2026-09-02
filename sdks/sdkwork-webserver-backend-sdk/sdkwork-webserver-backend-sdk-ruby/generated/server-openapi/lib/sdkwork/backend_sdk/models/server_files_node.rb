module Sdkwork
  module BackendSdk
    module Models
      class ServerFilesNode
              attr_accessor :id, :name, :host, :ssh_port, :status, :filesystem_root, :region

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @name = attributes['name']
                @host = attributes['host']
                @ssh_port = attributes['sshPort']
                @status = attributes['status']
                @filesystem_root = attributes['filesystemRoot']
                @region = attributes['region']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'name' => @name,
                  'host' => @host,
                  'sshPort' => @ssh_port,
                  'status' => @status,
                  'filesystemRoot' => @filesystem_root,
                  'region' => @region,
                }
              end
            end
    end
  end
end

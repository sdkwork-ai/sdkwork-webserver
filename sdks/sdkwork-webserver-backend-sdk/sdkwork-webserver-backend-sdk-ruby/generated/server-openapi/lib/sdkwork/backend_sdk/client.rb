module Sdkwork
  module BackendSdk
    class SdkworkBackendClient
      attr_reader :http, :application, :application_domain, :certificate, :domain, :application_source_version, :application_deployment, :certificate_distribution, :nginx, :server, :server_file, :agent, :audit
      def initialize(config)
        @http = Http::Client.new(config)
        @application = Api::ApplicationApi.new(@http)
        @application_domain = Api::ApplicationDomainApi.new(@http)
        @certificate = Api::CertificateApi.new(@http)
        @domain = Api::DomainApi.new(@http)
        @application_source_version = Api::ApplicationSourceVersionApi.new(@http)
        @application_deployment = Api::ApplicationDeploymentApi.new(@http)
        @certificate_distribution = Api::CertificateDistributionApi.new(@http)
        @nginx = Api::NginxApi.new(@http)
        @server = Api::ServerApi.new(@http)
        @server_file = Api::ServerFileApi.new(@http)
        @agent = Api::AgentApi.new(@http)
        @audit = Api::AuditApi.new(@http)
      end

      def set_api_key(api_key)
        @http.set_api_key(api_key)
        self
      end

      def set_auth_token(token)
        @http.set_auth_token(token)
        self
      end

      def set_access_token(token)
        @http.set_access_token(token)
        self
      end

      def set_header(key, value)
        @http.set_header(key, value)
        self
      end
    end
  end
end

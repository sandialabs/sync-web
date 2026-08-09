FROM nginx:1.30.2-alpine3.23@sha256:5f979dcfed4ce6461873f087e8c980d6e29b084b9e8776d9704a7e989b5f4898

COPY nginx.http.conf /etc/nginx/templates/nginx.http.conf
COPY nginx.tls.conf /etc/nginx/templates/nginx.tls.conf
COPY nginx.routes.conf /etc/nginx/includes/nginx.routes.inc
COPY static/ /usr/share/nginx/html/
COPY docker-entrypoint.sh /docker-entrypoint.sh

RUN chmod +x /docker-entrypoint.sh

EXPOSE 80 443

ENTRYPOINT ["/docker-entrypoint.sh"]
CMD ["nginx", "-g", "daemon off;"]

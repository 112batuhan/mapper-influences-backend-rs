ARG SURREAL_USER
ARG SURREAL_PASS
ARG SURREAL_URL
ARG CLIENT_ID
ARG CLIENT_SECRET
ARG REDIRECT_URI
ARG POST_LOGIN_REDIRECT_URI
ARG JWT_SECRET_KEY
ARG PORT
ARG ADMIN_PASSWORD

FROM rust:latest as rust-builder
WORKDIR /usr/src/mapper_influences_backend
COPY . .
RUN cargo build --release

FROM rust:slim
WORKDIR /usr/src/mapper_influences_backend
COPY --from=rust-builder /usr/src/mapper_influences_backend/target/release/mapper-influences-backend .
COPY --from=rust-builder /usr/src/mapper_influences_backend/src/elements-ui.html .
COPY --from=rust-builder /usr/src/mapper_influences_backend/src/graph-2d.html .
COPY --from=rust-builder /usr/src/mapper_influences_backend/src/graph-3d.html .

ENV SURREAL_USER=${SURREAL_USER}
ENV SURREAL_PASS=${SURREAL_PASS}
ENV SURREAL_URL=${SURREAL_URL}
ENV CLIENT_ID=${SURREAL_USER}
ENV CLIENT_SECRET=${CLIENT_SECRET}
ENV REDIRECT_URI=${REDIRECT_URI}
ENV POST_LOGIN_REDIRECT_URI=${POST_LOGIN_REDIRECT_URI}
ENV JWT_SECRET_KEY=${JWT_SECRET_KEY}
ENV PORT=${PORT}
ENV ADMIN_PASSWORD=${ADMIN_PASSWORD}

EXPOSE ${PORT}
ENTRYPOINT ["./mapper-influences-backend"]

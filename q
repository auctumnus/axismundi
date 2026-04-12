select
                lfm.id,
                lfm.family_id,
                lf.name as family_name,
                lf.code as family_code,
                l.name  as language_name,
                lfm.title
            from language_family_members lfm
            join language_families lf on lf.id = lfm.family_id
            left join languages l on l.id = lfm.language_id
            -- only families that contain the source language
            where lfm.family_id in (
                select family_id from language_family_members
                where language_id = '019d5abc-9165-73d2-9544-29da1ad96daa'
            )
            -- only families where the user has editor access
            and exists (
                select 1 from language_family_permissions lfp
                where lfp.family = lfm.family_id
                  and lfp."user" = '019d5aad-0e21-7526-8621-b01b4f78d734'
                  and lfp.permission >= 'editor'
            )
            -- exclude members that already have a sound change set
            and not exists (
                select 1 from sound_change_sets scs
                where scs.member_id = lfm.id
            )
            order by lf.name, lfm.title, l.name
